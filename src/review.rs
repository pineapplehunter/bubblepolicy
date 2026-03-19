use color_eyre::{Result, eyre::WrapErr};
use std::fs;

use crate::common::{Access, PolicyNode, PolicyTree};

pub fn run(
    file: &str,
    ro: &[String],
    rw: &[String],
    tmp: &[String],
    deny: &[String],
) -> Result<()> {
    let data =
        fs::read_to_string(file).with_context(|| format!("Failed to read file: {}", file))?;

    let mut trees: Vec<PolicyTree> = serde_json::from_str(&data).context("Failed to parse JSON")?;

    for tree in &mut trees {
        for path in ro {
            set_node_access(tree, path, Access::ReadOnly);
        }
        for path in rw {
            set_node_access(tree, path, Access::ReadWrite);
        }
        for path in tmp {
            set_node_access(tree, path, Access::Tmpfs);
        }
        for path in deny {
            set_node_access(tree, path, Access::Deny);
        }
    }

    let json = serde_json::to_string_pretty(&trees)?;
    fs::write(file, json).with_context(|| format!("Failed to write file: {}", file))?;

    eprintln!("Updated: {}", file);
    Ok(())
}

fn set_node_access(tree: &mut PolicyTree, path: &str, access: Access) {
    for node in &mut tree.entries {
        if set_node_access_recursive(node, path, &access) {
            return;
        }
    }
}

#[allow(dead_code)]
fn set_node_access_recursive(node: &mut PolicyNode, path: &str, access: &Access) -> bool {
    if node.path == path {
        node.access = access.clone();
        return true;
    }

    for child in &mut node.children {
        if set_node_access_recursive(child, path, access) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_node_access() {
        let mut tree = PolicyTree {
            entries: vec![PolicyNode {
                path: "/etc".to_string(),
                access: Access::ReadOnly,
                children: vec![PolicyNode {
                    path: "/etc/passwd".to_string(),
                    access: Access::ReadOnly,
                    children: vec![],
                }],
            }],
        };

        set_node_access(&mut tree, "/etc/passwd", Access::ReadWrite);
        assert_eq!(tree.entries[0].children[0].access, Access::ReadWrite);
    }

    #[test]
    fn test_set_node_access_root() {
        let mut tree = PolicyTree {
            entries: vec![PolicyNode {
                path: "/".to_string(),
                access: Access::Deny,
                children: vec![],
            }],
        };

        set_node_access(&mut tree, "/", Access::ReadOnly);
        assert_eq!(tree.entries[0].access, Access::ReadOnly);
    }

    #[test]
    fn test_set_node_access_nested() {
        let mut tree = PolicyTree {
            entries: vec![PolicyNode {
                path: "/".to_string(),
                access: Access::Deny,
                children: vec![PolicyNode {
                    path: "/bin".to_string(),
                    access: Access::ReadOnly,
                    children: vec![PolicyNode {
                        path: "/bin/ls".to_string(),
                        access: Access::ReadOnly,
                        children: vec![],
                    }],
                }],
            }],
        };

        set_node_access(&mut tree, "/bin/ls", Access::Tmpfs);
        let ls_node = &tree.entries[0].children[0].children[0];
        assert_eq!(ls_node.access, Access::Tmpfs);
    }

    #[test]
    fn test_set_node_access_multiple_paths() {
        let mut tree = PolicyTree {
            entries: vec![PolicyNode {
                path: "/etc".to_string(),
                access: Access::ReadOnly,
                children: vec![
                    PolicyNode {
                        path: "/etc/passwd".to_string(),
                        access: Access::ReadOnly,
                        children: vec![],
                    },
                    PolicyNode {
                        path: "/etc/shadow".to_string(),
                        access: Access::ReadOnly,
                        children: vec![],
                    },
                ],
            }],
        };

        set_node_access(&mut tree, "/etc/passwd", Access::ReadWrite);
        set_node_access(&mut tree, "/etc/shadow", Access::Deny);

        assert_eq!(tree.entries[0].children[0].access, Access::ReadWrite);
        assert_eq!(tree.entries[0].children[1].access, Access::Deny);
    }
}
