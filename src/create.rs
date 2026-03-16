use color_eyre::{
    eyre::{bail, WrapErr},
    Result,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Access {
    Deny,
    ReadOnly,
    ReadWrite,
    Tmpfs,
}

impl Access {
    pub fn is_allowed(&self) -> bool {
        !matches!(self, Access::Deny)
    }

    pub fn is_tmpfs(&self) -> bool {
        matches!(self, Access::Tmpfs)
    }

    pub fn is_write(&self) -> bool {
        matches!(self, Access::ReadWrite | Access::Tmpfs)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEntry {
    pub path: String,
    pub access: Access,
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

fn dedup_entries(entries: &[PolicyEntry]) -> Vec<PolicyEntry> {
    use std::collections::HashMap;

    #[derive(Clone)]
    struct TreeNode {
        path: String,
        access: Option<Access>,
        is_leaf: bool,
        children: HashMap<String, TreeNode>,
    }

    let mut root = TreeNode {
        path: "/".to_string(),
        access: None,
        is_leaf: false,
        children: HashMap::new(),
    };

    for entry in entries {
        if !entry.access.is_allowed() {
            continue;
        }

        let path = &entry.path;

        if path == "/" {
            root.access = Some(entry.access.clone());
            continue;
        }

        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut root;

        for (i, part) in parts.iter().enumerate() {
            let is_last = i == parts.len() - 1;
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
                        access: None,
                        is_leaf: is_last,
                        children: HashMap::new(),
                    },
                );
            }

            let child = current.children.get_mut(*part).unwrap();

            if is_last {
                child.access = Some(entry.access.clone());
                child.is_leaf = true;
            }

            current = child;
        }
    }

    // Simple recursive algorithm - emit only explicit nodes that differ from parent
    fn collect_deduped(
        node: &TreeNode,
        parent_access: Option<&Access>,
        result: &mut Vec<PolicyEntry>,
    ) {
        // Check if all children have same explicit access (can collapse)
        let mut child_accesses: Vec<&Access> = Vec::new();
        for child in node.children.values() {
            if let Some(access) = &child.access {
                child_accesses.push(access);
            }
        }

        // Check if all children have same access
        let can_collapse =
            !child_accesses.is_empty() && child_accesses.iter().all(|a| *a == child_accesses[0]);

        if can_collapse {
            // All children have same explicit access - collapse to parent
            // Emit this node if it has explicit access different from parent
            if let Some(access) = &node.access {
                let diff = match parent_access {
                    None => true,
                    Some(p) => !is_same_access(p, access),
                };
                if diff {
                    result.push(PolicyEntry {
                        path: node.path.clone(),
                        access: access.clone(),
                    });
                }
            } else {
                // No explicit on this node, but can collapse children to it
                let collapsed = child_accesses[0];
                let diff = match parent_access {
                    None => true,
                    Some(p) => !is_same_access(p, collapsed),
                };
                if diff {
                    result.push(PolicyEntry {
                        path: node.path.clone(),
                        access: collapsed.clone(),
                    });
                }
            }
        } else {
            // Can't collapse - pass this node's access to children
            let inherited = node.access.as_ref().or(parent_access);
            for child in node.children.values() {
                collect_deduped(child, inherited, result);
            }

            // Emit this node if explicit and different from parent
            if let Some(access) = &node.access {
                let diff = match parent_access {
                    None => true,
                    Some(p) => !is_same_access(p, access),
                };
                if diff {
                    result.push(PolicyEntry {
                        path: node.path.clone(),
                        access: access.clone(),
                    });
                }
            }
        }
    }

    fn is_same_access(a: &Access, b: &Access) -> bool {
        matches!(
            (a, b),
            (Access::Deny, Access::Deny)
                | (Access::ReadOnly, Access::ReadOnly)
                | (Access::ReadWrite, Access::ReadWrite)
                | (Access::Tmpfs, Access::Tmpfs)
        )
    }

    let mut result = Vec::new();
    collect_deduped(&root, None, &mut result);

    result.sort_by(|a, b| a.path.cmp(&b.path));

    result
}

fn generate_wrapper(policy: &Policy, binary: &str) -> Result<String> {
    // Deduplicate entries - remove children that have same access as parent
    let entries = dedup_entries(&policy.entries);
    eprintln!("Deduplicated entries: {}", entries.len());

    // Just use the deduped entries directly
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

    // Sort
    ro_binds.sort();
    rw_binds.sort();
    tmpfs_mounts.sort();

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

    // Add tmpfs mounts
    for path in tmpfs_mounts {
        bwrap_args.push(format!("    --tmpfs {} \\", path));
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
