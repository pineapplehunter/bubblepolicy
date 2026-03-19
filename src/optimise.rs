use color_eyre::{Result, eyre::WrapErr};
use std::fs;

use crate::common::{Access, PolicyNode, PolicyTree};

#[derive(Debug, Clone)]
pub struct PolicyEntry {
    pub path: String,
    pub access: Access,
}

pub fn run(file: &str) -> Result<()> {
    let data =
        fs::read_to_string(file).with_context(|| format!("Failed to read file: {}", file))?;

    let trees: Vec<PolicyTree> = serde_json::from_str(&data).context("Failed to parse JSON")?;

    let optimised: Vec<PolicyTree> = trees
        .into_iter()
        .map(optimise_tree)
        .filter(|t| !t.entries.is_empty())
        .collect();

    let json = serde_json::to_string_pretty(&optimised)?;
    fs::write(file, json).with_context(|| format!("Failed to write file: {}", file))?;

    eprintln!("Optimised: {}", file);
    Ok(())
}

pub fn optimise_tree(tree: PolicyTree) -> PolicyTree {
    let entries: Vec<PolicyEntry> = tree.entries.iter().flat_map(node_to_entries).collect();

    let deduped = entries_to_tree(&entries);

    let flattened = flatten_containers(&deduped);

    // Sort by path
    let mut sorted = flattened;
    sort_nodes(&mut sorted);

    PolicyTree { entries: sorted }
}

fn sort_nodes(nodes: &mut [PolicyNode]) {
    nodes.sort_by(|a, b| a.path.cmp(&b.path));
    for node in nodes.iter_mut() {
        sort_nodes(&mut node.children);
    }
}

fn flatten_containers(nodes: &[PolicyNode]) -> Vec<PolicyNode> {
    nodes.iter().filter_map(flatten_node).collect()
}

fn flatten_node(node: &PolicyNode) -> Option<PolicyNode> {
    if node.children.is_empty() {
        return Some(node.clone());
    }

    // Recursively flatten children
    let flattened_children: Vec<PolicyNode> =
        node.children.iter().filter_map(flatten_node).collect();

    if flattened_children.is_empty() {
        return None;
    }

    // If this node's access is Deny (the default) and all children have
    // different access, we can skip this intermediate node
    if node.access == Access::Deny {
        return Some(PolicyNode {
            path: node.path.clone(),
            access: node.access.clone(),
            children: flattened_children,
        });
    }

    // If node has non-Deny access (like Tmpfs) and has children,
    // check if all children are just intermediate containers
    // If so, promote the deepest children to be direct children
    let all_containers = flattened_children
        .iter()
        .all(|c| c.access == node.access && !c.children.is_empty());

    if all_containers && !flattened_children.is_empty() {
        // Flatten: collect all grandchildren as direct children
        let mut new_children = Vec::new();
        for child in flattened_children {
            new_children.extend(child.children);
        }
        return Some(PolicyNode {
            path: node.path.clone(),
            access: node.access.clone(),
            children: new_children,
        });
    }

    Some(PolicyNode {
        path: node.path.clone(),
        access: node.access.clone(),
        children: flattened_children,
    })
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

pub fn entries_to_tree(entries: &[PolicyEntry]) -> Vec<PolicyNode> {
    use std::collections::HashMap;

    #[derive(Clone)]
    struct TreeNode {
        path: String,
        access: Option<Access>,
        children: HashMap<String, TreeNode>,
    }

    let mut root = TreeNode {
        path: "/".to_string(),
        access: None,
        children: HashMap::new(),
    };

    let mut has_root_deny = false;

    for entry in entries {
        let path = &entry.path;
        if path == "/" {
            if entry.access == Access::Deny {
                has_root_deny = true;
            } else if entry.access.is_allowed() {
                root.access = Some(entry.access.clone());
            }
            continue;
        }

        if !entry.access.is_allowed() {
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
                        children: HashMap::new(),
                    },
                );
            }

            let child = current.children.get_mut(*part).unwrap();

            if is_last {
                child.access = Some(entry.access.clone());
            }

            current = child;
        }
    }

    if has_root_deny {
        return vec![PolicyNode {
            path: "/".to_string(),
            access: Access::Deny,
            children: vec![],
        }];
    }

    fn collect_trees(
        node: &TreeNode,
        parent_access: Option<&Access>,
        result: &mut Vec<PolicyNode>,
    ) {
        let inherited = node.access.as_ref().or(parent_access);

        for child in node.children.values() {
            if let Some(access) = &child.access {
                if *access == Access::Deny {
                    continue;
                }
                let children = collect_child_trees(child, access);
                result.push(PolicyNode {
                    path: child.path.clone(),
                    access: access.clone(),
                    children,
                });
            } else if let Some(inherited) = inherited {
                if *inherited == Access::Deny {
                    continue;
                }
                let children = collect_child_trees(child, inherited);
                if !children.is_empty() {
                    result.push(PolicyNode {
                        path: child.path.clone(),
                        access: inherited.clone(),
                        children,
                    });
                }
            } else {
                collect_trees(child, None, result);
            }
        }
    }

    fn collect_child_trees(node: &TreeNode, parent_access: &Access) -> Vec<PolicyNode> {
        let mut result = Vec::new();

        for child in node.children.values() {
            if let Some(access) = &child.access {
                if *access == Access::Deny {
                    continue;
                }
                let children = collect_child_trees(child, access);
                result.push(PolicyNode {
                    path: child.path.clone(),
                    access: access.clone(),
                    children,
                });
            } else {
                if *parent_access == Access::Deny {
                    continue;
                }
                let children = collect_child_trees(child, parent_access);
                if !children.is_empty() {
                    result.push(PolicyNode {
                        path: child.path.clone(),
                        access: parent_access.clone(),
                        children,
                    });
                }
            }
        }

        result
    }

    let mut result = Vec::new();

    if let Some(access) = &root.access {
        if *access != Access::Deny {
            let children = collect_child_trees(&root, access);
            result.push(PolicyNode {
                path: "/".to_string(),
                access: access.clone(),
                children,
            });
        }
    } else {
        collect_trees(&root, None, &mut result);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(path: &str, access: Access) -> PolicyEntry {
        PolicyEntry {
            path: path.to_string(),
            access,
        }
    }

    #[test]
    fn test_collapse_rule_1() {
        let entries = vec![
            make_entry("/", Access::ReadOnly),
            make_entry("/aaa", Access::ReadOnly),
            make_entry("/aaa/bbb", Access::ReadOnly),
            make_entry("/aaa/ccc", Access::ReadOnly),
        ];

        let tree = entries_to_tree(&entries);

        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].path, "/");
        assert_eq!(tree[0].access, Access::ReadOnly);
    }

    #[test]
    fn test_empty_entries() {
        let entries: Vec<PolicyEntry> = vec![];
        let tree = entries_to_tree(&entries);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_multiple_trees() {
        let entries = vec![
            make_entry("/a", Access::ReadOnly),
            make_entry("/b", Access::ReadOnly),
        ];
        let tree = entries_to_tree(&entries);
        assert_eq!(tree.len(), 2);
    }

    #[test]
    fn test_optimise_tree_empty() {
        let tree = PolicyTree { entries: vec![] };
        let result = optimise_tree(tree);
        assert!(result.entries.is_empty());
    }
}
