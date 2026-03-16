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
    let mut root_was_in_input = false;

    for entry in entries {
        // Track if root was in input
        if entry.path == "/" {
            root_was_in_input = true;
        }

        if !entry.access.is_allowed() {
            continue;
        }

        let path = &entry.path;

        // Handle root specially
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

    fn get_effective_access(node: &TreeNode) -> Option<Access> {
        if let Some(access) = &node.access {
            return Some(access.clone());
        }

        if node.children.is_empty() {
            return None;
        }

        let mut child_accesses: Vec<Access> = Vec::new();
        for child in node.children.values() {
            if let Some(access) = get_effective_access(child) {
                child_accesses.push(access);
            }
        }

        if child_accesses.is_empty() {
            None
        } else if child_accesses.iter().all(|a| *a == child_accesses[0]) {
            // ALL children have same access - can collapse
            Some(child_accesses[0].clone())
        } else {
            // Children have different access - don't collapse
            None
        }
    }

    fn collect_deduped(
        node: &TreeNode,
        ancestor_access: Option<&Access>,
        result: &mut Vec<PolicyEntry>,
    ) {
        // Get effective access for this node
        let effective = get_effective_access(node);

        // Determine what children will inherit
        let child_ancestor = if let Some(access) = &effective {
            // Decide whether to emit this node
            // We emit if:
            // 1. Root has explicit access, OR
            // 2. Node has explicit access and differs from ancestor, OR
            // 3. Node has NO explicit but has computed access (all children same)
            //    AND differs from ancestor (or no ancestor)
            let should_emit = if node.path == "/" {
                node.access.is_some()
            } else if node.access.is_some() {
                // Has explicit access - emit if different from ancestor
                ancestor_access.map_or(true, |a| *a != *access)
            } else {
                // No explicit - has computed access (all children same)
                // Emit if different from ancestor (or no ancestor - this becomes the root)
                ancestor_access.map_or(true, |a| *a != *access)
            };

            if should_emit {
                result.push(PolicyEntry {
                    path: node.path.clone(),
                    access: access.clone(),
                });
                // Children inherit from this node
                Some(access)
            } else {
                // Children inherit from ancestor
                ancestor_access
            }
        } else {
            // No effective access at all
            ancestor_access
        };

        // Process children
        for child in node.children.values() {
            collect_deduped(child, child_ancestor, result);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    // Rule 1: All children have same access -> collapse to parent
    // /:ro, /aaa:ro, /aaa/bbb:ro, /aaa/ccc:ro -> /:ro
    #[test]
    fn test_collapse_rule_1() {
        let entries = vec![
            PolicyEntry {
                path: "/".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/aaa".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/aaa/bbb".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/aaa/ccc".to_string(),
                access: Access::ReadOnly,
            },
        ];

        let result = dedup_entries(&entries);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "/");
        assert_eq!(result[0].access, Access::ReadOnly);
    }

    // Rule 2: Children have different access -> keep differing ones
    // /:ro, /aaa:ro, /aaa/bbb:ro, /aaa/ccc:rw -> /:ro, /aaa/ccc:rw
    #[test]
    fn test_collapse_rule_2() {
        let entries = vec![
            PolicyEntry {
                path: "/".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/aaa".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/aaa/bbb".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/aaa/ccc".to_string(),
                access: Access::ReadWrite,
            },
        ];

        let result = dedup_entries(&entries);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].path, "/");
        assert_eq!(result[0].access, Access::ReadOnly);
        assert_eq!(result[1].path, "/aaa/ccc");
        assert_eq!(result[1].access, Access::ReadWrite);
    }

    // Rule 3: Deny at root, children have different access
    // /:deny, /aaa:ro, /aaa/bbb:tmp, /aaa/ccc/ddd:rw -> /aaa:ro, /aaa/bbb:tmp, /aaa/ccc/ddd:rw
    #[test]
    fn test_collapse_rule_3() {
        let entries = vec![
            PolicyEntry {
                path: "/".to_string(),
                access: Access::Deny,
            },
            PolicyEntry {
                path: "/aaa".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/aaa/bbb".to_string(),
                access: Access::Tmpfs,
            },
            PolicyEntry {
                path: "/aaa/ccc/ddd".to_string(),
                access: Access::ReadWrite,
            },
        ];

        let result = dedup_entries(&entries);

        // Only allowed entries should appear (Deny entries are skipped)
        eprintln!("Rule 3 result: {:?}", result);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].path, "/aaa");
        assert_eq!(result[0].access, Access::ReadOnly);
        assert_eq!(result[1].path, "/aaa/bbb");
        assert_eq!(result[1].access, Access::Tmpfs);
        assert_eq!(result[2].path, "/aaa/ccc/ddd");
        assert_eq!(result[2].access, Access::ReadWrite);
    }

    // Additional test: siblings with same access collapse
    #[test]
    fn test_siblings_collapse() {
        let entries = vec![
            PolicyEntry {
                path: "/a/1".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/a/2".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/a/3".to_string(),
                access: Access::ReadOnly,
            },
        ];

        let result = dedup_entries(&entries);

        // All siblings have same access - collapse to parent
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "/a");
        assert_eq!(result[0].access, Access::ReadOnly);
    }

    // Additional test: mixed access among siblings
    #[test]
    fn test_siblings_mixed_access() {
        let entries = vec![
            PolicyEntry {
                path: "/a/1".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/a/2".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/a/3".to_string(),
                access: Access::ReadWrite,
            },
        ];

        let result = dedup_entries(&entries);

        // /a/1 and /a/2 collapse to /a with ReadOnly
        // /a/3 stays as /a/3 with ReadWrite
        assert_eq!(result.len(), 2);
    }
}
