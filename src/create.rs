use color_eyre::{
    eyre::{bail, WrapErr},
    Result,
};
use std::fs;
use std::path::Path;

use crate::common::{Access, PolicyNode, PolicyTree};

#[derive(Debug, Clone)]
pub struct PolicyEntry {
    pub path: String,
    pub access: Access,
}

pub fn run(policy: &str, binary: &str) -> Result<()> {
    if !Path::new(policy).exists() {
        bail!("Policy file not found: {}", policy);
    }

    let policy_content = fs::read_to_string(policy).context("Failed to read policy file")?;

    let trees: Vec<PolicyTree> =
        serde_json::from_str(&policy_content).context("Failed to parse policy JSON")?;

    let entries: Vec<PolicyEntry> = trees.iter().flat_map(tree_to_entries).collect();

    println!("Creating bubblewrap wrapper for: {}", binary);
    println!("Policy entries: {}", entries.len());

    let script = generate_wrapper(&entries, binary)?;

    println!("{}", script);

    Ok(())
}

fn tree_to_entries(tree: &PolicyTree) -> Vec<PolicyEntry> {
    tree.entries.iter().flat_map(node_to_entries).collect()
}

fn node_to_entries(node: &PolicyNode) -> Vec<PolicyEntry> {
    let mut entries = vec![PolicyEntry {
        path: node.path.clone(),
        access: node.access.clone(),
    }];

    for child in &node.children {
        entries.extend(node_to_entries(child));
    }

    entries
}

fn generate_wrapper(entries: &[PolicyEntry], binary: &str) -> Result<String> {
    let entries = dedup_entries(entries);
    eprintln!("Deduplicated entries: {}", entries.len());

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

fn dedup_entries(entries: &[PolicyEntry]) -> Vec<PolicyEntry> {
    use std::collections::HashMap;

    #[derive(Clone)]
    #[allow(dead_code)]
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

            current = current.children.get_mut(*part).unwrap();
            if is_last {
                current.access = Some(entry.access.clone());
            }
        }
    }

    fn collect_deduped(
        node: &TreeNode,
        parent_access: Option<&Access>,
        result: &mut Vec<PolicyEntry>,
    ) {
        let child_accesses: Vec<&Access> = node
            .children
            .values()
            .filter_map(|c| c.access.as_ref())
            .collect();

        let can_collapse =
            !child_accesses.is_empty() && child_accesses.iter().all(|a| *a == child_accesses[0]);

        if can_collapse {
            // Only collapse if this node was explicitly set in original entries
            if let Some(access) = &node.access {
                let diff = match parent_access {
                    None => true,
                    Some(p) => !std::ptr::eq(p as *const Access, access as *const Access),
                };
                if diff {
                    result.push(PolicyEntry {
                        path: node.path.clone(),
                        access: access.clone(),
                    });
                }
            } else {
                // Node wasn't in original entries, don't collapse - process children instead
                for child in node.children.values() {
                    collect_deduped(child, parent_access, result);
                }
            }
        } else {
            let inherited = node.access.as_ref().or(parent_access);
            for child in node.children.values() {
                collect_deduped(child, inherited, result);
            }

            if let Some(access) = &node.access {
                let diff = match parent_access {
                    None => true,
                    Some(p) => !std::ptr::eq(p as *const Access, access as *const Access),
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

    let mut result = Vec::new();
    collect_deduped(&root, None, &mut result);

    result.sort_by(|a, b| a.path.cmp(&b.path));

    result
}
