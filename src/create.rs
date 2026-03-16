use color_eyre::{
    eyre::{bail, WrapErr},
    Result,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEntry {
    pub path: String,
    pub allowed: bool,
    pub read: bool,
    pub write: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Policy {
    pub entries: Vec<PolicyEntry>,
}

pub fn run(policy: Option<&str>, output: Option<&str>, binary: Option<&str>) -> Result<()> {
    let policy_path = policy.unwrap_or("policy.json");
    let binary_path = binary.unwrap_or("/bin/sh");

    if !Path::new(policy_path).exists() {
        bail!("Policy file not found: {}", policy_path);
    }

    let policy_content = fs::read_to_string(policy_path).context("Failed to read policy file")?;
    let policy: Policy =
        serde_json::from_str(&policy_content).context("Failed to parse policy JSON")?;

    println!("Creating bubblewrap wrapper for: {}", binary_path);
    println!("Policy entries: {}", policy.entries.len());

    let script = generate_wrapper(&policy, binary_path)?;

    if let Some(out_path) = output {
        fs::write(out_path, &script).context("Failed to write output file")?;
        println!("Wrapper written to: {}", out_path);
    } else {
        println!("{}", script);
    }

    Ok(())
}

fn generate_wrapper(policy: &Policy, binary: &str) -> Result<String> {
    // Collect allowed paths and separate by write permission
    let mut all_paths: Vec<PathBuf> = Vec::new();
    let mut write_paths: HashSet<PathBuf> = HashSet::new();

    for entry in &policy.entries {
        if !entry.allowed {
            continue;
        }

        let path = PathBuf::from(&entry.path);
        all_paths.push(path.clone());

        if entry.write {
            write_paths.insert(path);
        }
    }

    // Sort and dedup
    all_paths.sort();
    all_paths.dedup();

    // Build a map of directory -> list of paths in that directory
    let mut dir_to_paths: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();

    for path in &all_paths {
        if let Some(parent) = path.parent() {
            let parent_entry = dir_to_paths.entry(parent.to_path_buf()).or_default();
            parent_entry.push(path.clone());
        }
    }

    // Find common ancestors for directories with multiple children
    let mut dir_parents: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    
    for (dir, _paths) in &dir_to_paths {
        if let Some(parent) = dir.parent() {
            if !parent.as_os_str().is_empty() {
                dir_parents.entry(parent.to_path_buf()).or_default().push(dir.clone());
            }
        }
    }

    // Decide what to bind
    let mut ro_binds = Vec::new();
    let mut rw_binds = Vec::new();
    let mut covered: HashSet<PathBuf> = HashSet::new();

    // First: bind common ancestors if they have multiple child directories
    for (ancestor, children) in &dir_parents {
        if children.len() > 1 {
            // This ancestor has multiple subdirectories with files
            // Check if any of those files have write permission
            let mut has_write = false;
            for child_dir in children {
                if let Some(files) = dir_to_paths.get(child_dir) {
                    for file in files {
                        if write_paths.contains(file) {
                            has_write = true;
                            break;
                        }
                    }
                }
                if has_write {
                    break;
                }
            }

            let ancestor_str = ancestor.display().to_string();
            if has_write {
                rw_binds.push(ancestor_str);
            } else {
                ro_binds.push(ancestor_str);
            }

            // Mark all files under these directories as covered
            for child_dir in children {
                if let Some(files) = dir_to_paths.get(child_dir) {
                    for file in files {
                        covered.insert(file.clone());
                    }
                }
            }
        }
    }

    // Second: bind individual directories with multiple files
    for (dir, files) in &dir_to_paths {
        // Skip if already covered
        if files.iter().all(|f| covered.contains(f)) {
            continue;
        }

        let dir_str = dir.display().to_string();
        let has_write = files.iter().any(|f| write_paths.contains(f));

        if has_write {
            rw_binds.push(dir_str);
        } else {
            ro_binds.push(dir_str);
        }

        for file in files {
            covered.insert(file.clone());
        }
    }

    // Third: add remaining individual files not covered
    for path in &all_paths {
        if !covered.contains(path) {
            let path_str = path.display().to_string();
            if write_paths.contains(path) {
                rw_binds.push(path_str);
            } else {
                ro_binds.push(path_str);
            }
        }
    }

    // Remove duplicates and sort
    ro_binds.sort();
    ro_binds.dedup();
    rw_binds.sort();
    rw_binds.dedup();

    // Build bwrap arguments
    let mut bwrap_args = Vec::new();

    // Add read-only binds
    for path in ro_binds {
        bwrap_args.push(format!("    --ro-bind {} {} \\", path, path));
    }

    // Add read-write binds
    for path in rw_binds {
        bwrap_args.push(format!("    --bind {} {} \\", path, path));
    }

    // Add standard system mounts
    bwrap_args.push("    --proc /proc \\".to_string());
    bwrap_args.push("    --dev /dev \\".to_string());
    bwrap_args.push("    --tmpfs /run \\".to_string());
    bwrap_args.push("    --tmpfs /tmp \\".to_string());

    // Join arguments
    let args_str = bwrap_args.join("\n");

    // Remove trailing backslash from last argument
    let args_str = if args_str.ends_with(" \\") {
        args_str[..args_str.len() - 2].to_string()
    } else {
        args_str
    };

    let script = format!(
        r#"#!/bin/bash
# Bubblewrap sandbox wrapper generated by myjail
# Binary: {}
# Policy entries: {}

set -e

# Check if bwrap is available
if ! command -v bwrap &> /dev/null; then
    echo "Error: bubblewrap (bwrap) is not installed"
    exit 1
fi

# Create the sandbox
exec bwrap \
{}  \
    -i \
    --chdir / \
    {} "$@"
"#,
        binary,
        policy.entries.len(),
        args_str,
        binary
    );

    Ok(script)
}
