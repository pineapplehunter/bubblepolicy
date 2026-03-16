use color_eyre::{eyre::bail, Result};
use nix::sys::ptrace;
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::fs;
use std::os::unix::process::CommandExt;
use syscall_numbers::native as sysnum;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileAccess {
    pub path: String,
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceOutput {
    pub files: Vec<FileAccess>,
}

pub fn run(cmd: &[String], output: Option<&str>) -> Result<()> {
    if cmd.is_empty() {
        bail!("No command provided");
    }

    let binary = &cmd[0];
    let args = &cmd[1..];

    eprintln!("Tracing: {} {:?}", binary, args);

    let file_accesses = trace_command(binary, args)?;

    let trace_output = TraceOutput {
        files: file_accesses,
    };

    let json = serde_json::to_string_pretty(&trace_output)?;

    if let Some(output_path) = output {
        fs::write(output_path, &json)?;
        eprintln!("Trace written to: {}", output_path);
    } else {
        println!("{}", json);
    }

    Ok(())
}

fn trace_command(binary: &str, args: &[String]) -> Result<Vec<FileAccess>> {
    let mut file_map: BTreeMap<String, FileAccess> = BTreeMap::new();

    let pid = unsafe { fork() }?;

    match pid {
        ForkResult::Parent { child } => {
            trace_child(child, &mut file_map)?;
        }
        ForkResult::Child => {
            ptrace::traceme()?;
            let _ = std::process::Command::new(binary).args(args).exec();
        }
    }

    let mut files: Vec<FileAccess> = file_map.into_values().collect();
    files.sort();

    Ok(files)
}

fn trace_child(child: Pid, file_map: &mut BTreeMap<String, FileAccess>) -> Result<()> {
    // Wait for initial stop
    match waitpid(child, None)? {
        WaitStatus::Stopped(_, Signal::SIGSTOP) => {}
        WaitStatus::Exited(_, _) | WaitStatus::Signaled(_, _, _) => return Ok(()),
        _ => {}
    }

    // Set options to trace fork/vfork/clone/exec
    ptrace::setoptions(
        child,
        nix::sys::ptrace::Options::PTRACE_O_TRACEFORK
            | nix::sys::ptrace::Options::PTRACE_O_TRACEVFORK
            | nix::sys::ptrace::Options::PTRACE_O_TRACECLONE
            | nix::sys::ptrace::Options::PTRACE_O_TRACEEXEC,
    )?;

    // Continue the child
    ptrace::syscall(child, None)?;

    // Main trace loop
    loop {
        let status = waitpid(child, None)?;

        match status {
            WaitStatus::Stopped(_, Signal::SIGTRAP) => {
                // Syscall entry or exit - check registers
                if let Ok(regs) = ptrace::getregs(child) {
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
