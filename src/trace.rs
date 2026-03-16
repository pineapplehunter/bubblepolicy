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
    pub read: bool,
    pub write: bool,
    pub execute: bool,
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

    let tree = files_to_tree(file_accesses);
    let trees: Vec<PolicyTree> = vec![tree];
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

    #[derive(Clone)]
    struct TreeNode {
        path: String,
        children: HashMap<String, TreeNode>,
    }

    let mut root = TreeNode {
        path: "/".to_string(),
        children: HashMap::new(),
    };

    for file in &files {
        let parts: Vec<&str> = file.path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut root;

        for (i, part) in parts.iter().enumerate() {
            let _is_last = i == parts.len() - 1;
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

    fn build_policy_tree(node: &TreeNode) -> Vec<PolicyNode> {
        let mut result = Vec::new();

        if node.children.is_empty() {
            return vec![PolicyNode {
                path: node.path.clone(),
                access: Access::ReadOnly,
                children: vec![],
            }];
        }

        let children: Vec<PolicyNode> =
            node.children.values().flat_map(build_policy_tree).collect();

        if !children.is_empty() {
            result.push(PolicyNode {
                path: node.path.clone(),
                access: Access::ReadOnly,
                children,
            });
        }

        result
    }

    let entries = build_policy_tree(&root);

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
    match waitpid(child, None)? {
        WaitStatus::Stopped(_, Signal::SIGSTOP) => {}
        WaitStatus::Exited(_, _) | WaitStatus::Signaled(_, _, _) => return Ok(()),
        _ => {}
    }

    ptrace::setoptions(
        child,
        nix::sys::ptrace::Options::PTRACE_O_TRACEFORK
            | nix::sys::ptrace::Options::PTRACE_O_TRACEVFORK
            | nix::sys::ptrace::Options::PTRACE_O_TRACECLONE
            | nix::sys::ptrace::Options::PTRACE_O_TRACEEXEC,
    )?;

    ptrace::syscall(child, None)?;

    loop {
        let status = waitpid(child, None)?;

        match status {
            WaitStatus::Stopped(_, Signal::SIGTRAP) => {
                if let Ok(regs) = ptrace::getregs(child) {
                    *syscall_count += 1;
                    let syscall_num = regs.orig_rax as i64;

                    match syscall_num {
                        sysnum::SYS_openat | sysnum::SYS_openat2 => {
                            let path = read_string(child, regs.rsi as *mut c_void);
                            if let Some(path) = path {
                                if let Some(path) = filter_path(&path) {
                                    let flags = regs.rdx as u32;
                                    update_file_access(file_map, &path, flags);
                                }
                            }
                        }
                        sysnum::SYS_open => {
                            let path = read_string(child, regs.rdi as *mut c_void);
                            if let Some(path) = path {
                                if let Some(path) = filter_path(&path) {
                                    let flags = regs.rsi as u32;
                                    update_file_access(file_map, &path, flags);
                                }
                            }
                        }
                        _ => {}
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

    let skip_prefixes = [
        "/proc/self/",
        "/proc/thread-self/",
        "/dev/pts/",
        "/usr/lib/locale/",
        "/usr/share/locale/",
    ];

    for prefix in skip_prefixes {
        if path.starts_with(prefix) {
            return None;
        }
    }

    if path.starts_with('/') {
        Some(path.to_string())
    } else {
        None
    }
}

fn update_file_access(file_map: &mut BTreeMap<String, FileAccess>, path: &str, flags: u32) {
    let read = (flags & 0x3) != 1;
    let write = (flags & 0x2) != 0 || (flags & 0x40) != 0 || (flags & 0x200) != 0;

    let entry = file_map.entry(path.to_string()).or_insert(FileAccess {
        path: path.to_string(),
        read: false,
        write: false,
        execute: false,
    });

    if read {
        entry.read = true;
    }
    if write {
        entry.write = true;
    }

    if !entry.read && !entry.write {
        entry.read = true;
    }
}
