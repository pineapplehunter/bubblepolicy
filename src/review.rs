use color_eyre::{eyre::WrapErr, Result};
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
}
