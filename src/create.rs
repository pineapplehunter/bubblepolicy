use color_eyre::{
    eyre::{bail, Context},
    Result,
};
use log::info;
use std::fs;
use std::path::Path;

use crate::common::{dedup_entries, parse_entries, PolicyEntry};

pub fn run(policy: &str, binary: &str) -> Result<()> {
    if !Path::new(policy).exists() {
        bail!("Policy file not found: {}", policy);
    }

    let policy_content = fs::read_to_string(policy).context("Failed to read policy file")?;

    let entries = parse_entries(&policy_content);

    info!("Creating bubblewrap wrapper for: {}", binary);
    info!("Policy entries: {}", entries.len());

    let script = generate_wrapper(&entries, binary)?;

    println!("{}", script);

    Ok(())
}

fn generate_wrapper(entries: &[PolicyEntry], binary: &str) -> Result<String> {
    let entries = dedup_entries(entries);
    info!("Deduplicated entries: {}", entries.len());

    let mut ro_binds = Vec::new();
    let mut rw_binds = Vec::new();
    let mut tmpfs_mounts = Vec::new();

    for entry in &entries {
        if !entry.access.is_allowed() {
            continue;
        }

        if entry.access.is_tmpfs() {
            tmpfs_mounts.push(entry.path.clone());
        } else if entry.access.is_write() {
            rw_binds.push(entry.path.clone());
        } else {
            ro_binds.push(entry.path.clone());
        }
    }

    ro_binds.sort();
    rw_binds.sort();
    tmpfs_mounts.sort();

    let mut bwrap_args = Vec::new();

    for path in &ro_binds {
        bwrap_args.push(format!("    --ro-bind {} {} ", path, path));
    }

    for path in &rw_binds {
        bwrap_args.push(format!("    --bind {} {} ", path, path));
    }

    for path in &tmpfs_mounts {
        bwrap_args.push(format!("    --tmpfs {} ", path));
    }

    let args_str = bwrap_args.join("\\\n");

    let script = format!(
        include_str!("template.sh"),
        binary = binary,
        entries = entries.len(),
        args_str = args_str
    );

    Ok(script)
}
