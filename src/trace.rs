use color_eyre::{eyre::bail, Result};
use nix::sys::ptrace;
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::fs;
use std::os::unix::process::CommandExt;
use syscall_numbers::native as sysnum;

use crate::common::{Access, PolicyNode, PolicyTree};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileAccess {
    pub path: String,
}

pub fn run(cmd: &[String], output: Option<&str>) -> Result<()> {
    if cmd.is_empty() {
        bail!("No command provided");
    }

    let binary = &cmd[0];
    let args = &cmd[1..];

    eprintln!("Tracing: {} {:?}", binary, args);

    let file_accesses = trace_command(binary, args)?;
    eprintln!("Found {} file accesses", file_accesses.len());

    // Split into absolute and relative paths
    let mut absolute_files = Vec::new();
    let mut relative_files = Vec::new();

    for fa in &file_accesses {
        if fa.path.starts_with('/') {
            absolute_files.push(fa.clone());
        } else {
            relative_files.push(fa.clone());
        }
    }

    let trees: Vec<PolicyTree> = vec![
        files_to_tree(absolute_files),
        files_to_tree_relative(relative_files),
    ];

    let json = serde_json::to_string_pretty(&trees)?;

    if let Some(output_path) = output {
        fs::write(output_path, &json)?;
        eprintln!("Trace written to: {}", output_path);
    } else {
        println!("{}", json);
    }

    Ok(())
}

fn files_to_tree(files: Vec<FileAccess>) -> PolicyTree {
    use std::collections::HashMap;

    let file_map: std::collections::BTreeMap<String, FileAccess> =
        files.into_iter().map(|f| (f.path.clone(), f)).collect();

    #[derive(Clone)]
    struct TreeNode {
        path: String,
        children: HashMap<String, TreeNode>,
    }

    let mut root = TreeNode {
        path: "/".to_string(),
        children: HashMap::new(),
    };

    for file in file_map.values() {
        let parts: Vec<&str> = file.path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut root;

        for part in parts.iter() {
            let child_path = if current.path == "/" {
                format!("/{}", part)
            } else {
                format!("{}/{}", current.path, part)
            };

            if !current.children.contains_key(*part) {
                current.children.insert(
                    part.to_string(),
                    TreeNode {
                        path: child_path,
                        children: HashMap::new(),
                    },
                );
            }

            current = current.children.get_mut(*part).unwrap();
        }
    }

    fn build_policy_tree(
        node: &TreeNode,
        file_map: &std::collections::BTreeMap<String, FileAccess>,
    ) -> Vec<PolicyNode> {
        let mut result = Vec::new();

        if node.children.is_empty() {
            let access = file_map
                .get(&node.path)
                .map(|_fa| Access::Deny)
                .unwrap_or(Access::Deny);
            return vec![PolicyNode {
                path: node.path.clone(),
                access,
                children: vec![],
            }];
        }

        let children: Vec<PolicyNode> = node
            .children
            .values()
            .flat_map(|c| build_policy_tree(c, file_map))
            .collect();

        if !children.is_empty() {
            result.push(PolicyNode {
                path: node.path.clone(),
                access: Access::Deny,
                children,
            });
        }

        result
    }

    let entries = build_policy_tree(&root, &file_map);

    PolicyTree { entries }
}

fn files_to_tree_relative(files: Vec<FileAccess>) -> PolicyTree {
    use std::collections::HashMap;
    use std::env;

    let cwd = env::current_dir().unwrap_or_default();
    let cwd_str = cwd.to_string_lossy().to_string();

    let file_map: std::collections::BTreeMap<String, FileAccess> =
        files.iter().map(|f| (f.path.clone(), f.clone())).collect();

    #[derive(Clone)]
    struct TreeNode {
        path: String,
        children: HashMap<String, TreeNode>,
    }

    let mut root = TreeNode {
        path: cwd_str.clone(),
        children: HashMap::new(),
    };

    for file in file_map.values() {
        let parts: Vec<&str> = file.path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut root;

        for part in parts.iter() {
            let child_path = if current.path == cwd_str {
                format!("{}/{}", cwd_str, part)
            } else {
                format!("{}/{}", current.path, part)
            };

            if !current.children.contains_key(*part) {
                current.children.insert(
                    part.to_string(),
                    TreeNode {
                        path: child_path,
                        children: HashMap::new(),
                    },
                );
            }

            current = current.children.get_mut(*part).unwrap();
        }
    }

    fn build_policy_tree(
        node: &TreeNode,
        file_map: &std::collections::BTreeMap<String, FileAccess>,
    ) -> Vec<PolicyNode> {
        let mut result = Vec::new();

        if node.children.is_empty() {
            let access = file_map
                .get(&node.path)
                .map(|_fa| Access::Deny)
                .unwrap_or(Access::Deny);
            return vec![PolicyNode {
                path: node.path.clone(),
                access,
                children: vec![],
            }];
        }

        let children: Vec<PolicyNode> = node
            .children
            .values()
            .flat_map(|c| build_policy_tree(c, file_map))
            .collect();

        if !children.is_empty() {
            result.push(PolicyNode {
                path: node.path.clone(),
                access: Access::Deny,
                children,
            });
        }

        result
    }

    let entries = build_policy_tree(&root, &file_map);

    PolicyTree { entries }
}

fn trace_command(binary: &str, args: &[String]) -> Result<Vec<FileAccess>> {
    let mut file_map: BTreeMap<String, FileAccess> = BTreeMap::new();
    let mut syscall_count: usize = 0;

    let pid = unsafe { fork() }?;

    match pid {
        ForkResult::Parent { child } => {
            trace_child(child, &mut file_map, &mut syscall_count)?;
        }
        ForkResult::Child => {
            ptrace::traceme()?;
            let _ = std::process::Command::new(binary).args(args).exec();
        }
    }

    eprintln!("Syscalls caught: {}", syscall_count);

    let mut files: Vec<FileAccess> = file_map.into_values().collect();
    files.sort();

    Ok(files)
}

fn trace_child(
    child: Pid,
    file_map: &mut BTreeMap<String, FileAccess>,
    syscall_count: &mut usize,
) -> Result<()> {
    // Wait for initial stop
    let status = waitpid(child, None)?;

    match status {
        WaitStatus::Stopped(_, Signal::SIGTRAP) => {}
        WaitStatus::Stopped(_, Signal::SIGSTOP) => {}
        WaitStatus::Exited(_, _) | WaitStatus::Signaled(_, _, _) => return Ok(()),
        _ => {
            return Ok(());
        }
    }

    // Set options before continuing - only trace the exec'd process
    ptrace::setoptions(
        child,
        nix::sys::ptrace::Options::PTRACE_O_TRACEEXEC
            | nix::sys::ptrace::Options::PTRACE_O_TRACECLONE,
    )?;

    // Continue and wait for exec
    ptrace::syscall(child, None)?;

    loop {
        let status = waitpid(child, None)?;

        match status {
            WaitStatus::Stopped(_, Signal::SIGTRAP) => {
                if let Ok(regs) = ptrace::getregs(child) {
                    let syscall_num = regs.orig_rax as i64;

                    if syscall_num == sysnum::SYS_openat || syscall_num == sysnum::SYS_openat2 {
                        *syscall_count += 1;
                        let path = read_string(child, regs.rsi as *mut c_void);
                        if let Some(path) = path {
                            if let Some(path) = filter_path(&path) {
                                file_map.entry(path.to_string()).or_insert(FileAccess {
                                    path: path.to_string(),
                                });
                            }
                        }
                    } else if syscall_num == sysnum::SYS_open {
                        *syscall_count += 1;
                        let path = read_string(child, regs.rdi as *mut c_void);
                        if let Some(path) = path {
                            if let Some(path) = filter_path(&path) {
                                file_map.entry(path.to_string()).or_insert(FileAccess {
                                    path: path.to_string(),
                                });
                            }
                        }
                    }
                }
                ptrace::syscall(child, None)?;
            }
            WaitStatus::Stopped(_, Signal::SIGSTOP) => {
                ptrace::syscall(child, None)?;
            }
            WaitStatus::Exited(_, code) => {
                eprintln!("Child exited with code: {}", code);
                break;
            }
            WaitStatus::Signaled(_, sig, _) => {
                eprintln!("Child killed by signal: {:?}", sig);
                break;
            }
            WaitStatus::PtraceEvent(_, _, _) => {
                ptrace::syscall(child, None)?;
            }
            _ => {
                ptrace::syscall(child, None)?;
            }
        }
    }

    Ok(())
}

fn read_string(pid: Pid, addr: *mut c_void) -> Option<String> {
    if addr.is_null() {
        return None;
    }

    let mut data = [0u8; 256];

    for offset in (0..256).step_by(std::mem::size_of::<usize>()) {
        let read_addr = unsafe { addr.byte_add(offset) };
        match ptrace::read(pid, read_addr) {
            Ok(val) => {
                for i in 0..std::mem::size_of::<usize>() {
                    let byte = (val >> (i * 8)) as u8;
                    data[offset + i] = byte;
                    if byte == 0 {
                        return Some(String::from_utf8_lossy(&data[..offset + i]).to_string());
                    }
                }
            }
            Err(_) => return None,
        }
    }

    None
}

fn filter_path(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }

    let skip_prefixes: [&str; 0] = [];

    for prefix in skip_prefixes {
        if path.starts_with(prefix) {
            return None;
        }
    }

    if path.starts_with('/') {
        Some(path.to_string())
    } else {
        // Also capture relative paths
        Some(path.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_access_sort() {
        let mut files = vec![
            FileAccess {
                path: "/b".to_string(),
            },
            FileAccess {
                path: "/a".to_string(),
            },
        ];
        files.sort();
        assert_eq!(files[0].path, "/a");
        assert_eq!(files[1].path, "/b");
    }

    #[test]
    fn test_filter_path_absolute() {
        let result = filter_path("/absolute/path");
        assert_eq!(result, Some("/absolute/path".to_string()));
    }

    #[test]
    fn test_filter_path_relative() {
        let result = filter_path("relative/path");
        assert_eq!(result, Some("relative/path".to_string()));
    }

    #[test]
    fn test_filter_path_empty() {
        let result = filter_path("");
        assert_eq!(result, None);
    }
}
