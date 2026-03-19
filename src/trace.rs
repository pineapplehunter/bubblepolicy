use color_eyre::{eyre::bail, Result};
use log::{info, warn};
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use strace_open_parser::parse_strace_output;

use crate::common::Access;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileAccess {
    pub path: String,
    pub access: Access,
}

pub fn run(cmd: &[String], output: Option<&str>) -> Result<()> {
    if cmd.is_empty() {
        bail!("No command provided");
    }

    let binary = &cmd[0];
    let args = &cmd[1..];

    let file_accesses = trace_command(binary, args)?;
    info!("Found {} file accesses", file_accesses.len());

    let output_content = file_accesses
        .iter()
        .map(|fa| format!("{} {}", fa.access.to_str(), fa.path))
        .collect::<Vec<_>>()
        .join("\n");

    if let Some(output_path) = output {
        fs::write(output_path, output_content)?;
        info!("Trace written to: {}", output_path);
    } else {
        println!("{}", output_content);
    }

    Ok(())
}

fn trace_command(binary: &str, args: &[String]) -> Result<Vec<FileAccess>> {
    info!("Running strace on: {} {:?}", binary, args);

    let tmpfile = tempfile::NamedTempFile::new()?;
    let tmppath = tmpfile.path();

    let mut cmd = Command::new("strace");
    cmd.arg("-e").arg("trace=open,openat,openat2");
    cmd.arg("-f");
    cmd.arg("-o").arg(tmppath);
    cmd.arg("--");
    cmd.arg(binary);
    cmd.args(args);

    let status = cmd.status()?;
    if !status.success() {
        warn!("strace exited with non-zero status: {}", status);
    }

    let content = fs::read_to_string(tmppath)?;
    let file_accesses = parse_strace_output(&content);
    info!(
        "Parsed {} file accesses from strace output",
        file_accesses.len()
    );

    let mut file_map: BTreeMap<String, FileAccess> = BTreeMap::new();
    for fa in file_accesses {
        if !fa.path.is_empty() {
            let converted = FileAccess {
                path: fa.path,
                access: convert_access(fa.access),
            };
            file_map.entry(converted.path.clone()).or_insert(converted);
        }
    }

    let mut files: Vec<FileAccess> = file_map.into_values().collect();
    files.sort();

    Ok(files)
}

fn convert_access(access: strace_open_parser::Access) -> Access {
    match access {
        strace_open_parser::Access::ReadOnly => Access::ReadOnly,
        strace_open_parser::Access::ReadWrite => Access::ReadWrite,
        strace_open_parser::Access::Tmpfs => Access::Tmpfs,
        strace_open_parser::Access::Deny => Access::Deny,
    }
}
