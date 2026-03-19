use color_eyre::{eyre::bail, Result};
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use strace_open_parser::parse_strace_output;

use crate::common::{Access, PolicyNode, PolicyTree};

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

    eprintln!("Tracing: {} {:?}", binary, args);

    let file_accesses = trace_command(binary, args)?;
    eprintln!("Found {} file accesses", file_accesses.len());

    let mut absolute_files = Vec::new();
    let mut relative_files = Vec::new();

    for fa in &file_accesses {
        if fa.path.starts_with('/') {
            absolute_files.push(fa.clone());
        } else {
            relative_files.push(fa.clone());
        }
    }

    let trees: Vec<PolicyTree> = vec![
        files_to_tree(absolute_files),
        files_to_tree_relative(relative_files),
    ];

    let json = serde_json::to_string_pretty(&trees)?;

    if let Some(output_path) = output {
        fs::write(output_path, &json)?;
        eprintln!("Trace written to: {}", output_path);
    } else {
        println!("{}", json);
    }

    Ok(())
}

fn files_to_tree(files: Vec<FileAccess>) -> PolicyTree {
    use std::collections::HashMap;

    let file_map: std::collections::BTreeMap<String, FileAccess> =
        files.into_iter().map(|f| (f.path.clone(), f)).collect();

    #[derive(Clone)]
    struct TreeNode {
        path: String,
        children: HashMap<String, TreeNode>,
    }

    let mut root = TreeNode {
        path: "/".to_string(),
        children: HashMap::new(),
    };

    for file in file_map.values() {
        let parts: Vec<&str> = file.path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut root;

        for part in parts.iter() {
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
                        children: HashMap::new(),
                    },
                );
            }

            current = current.children.get_mut(*part).unwrap();
        }
    }

    fn build_policy_tree(
        node: &TreeNode,
        file_map: &std::collections::BTreeMap<String, FileAccess>,
    ) -> Vec<PolicyNode> {
        let mut result = Vec::new();

        if node.children.is_empty() {
            let access = file_map
                .get(&node.path)
                .map(|fa| fa.access.clone())
                .unwrap_or(Access::Deny);
            return vec![PolicyNode {
                path: node.path.clone(),
                access,
                children: vec![],
            }];
        }

        let children: Vec<PolicyNode> = node
            .children
            .values()
            .flat_map(|c| build_policy_tree(c, file_map))
            .collect();

        if !children.is_empty() {
            result.push(PolicyNode {
                path: node.path.clone(),
                access: Access::Deny,
                children,
            });
        }

        result
    }

    let entries = build_policy_tree(&root, &file_map);

    PolicyTree { entries }
}

fn files_to_tree_relative(files: Vec<FileAccess>) -> PolicyTree {
    use std::collections::HashMap;
    use std::env;

    let cwd = env::current_dir().unwrap_or_default();
    let cwd_str = cwd.to_string_lossy().to_string();

    let file_map: std::collections::BTreeMap<String, FileAccess> =
        files.iter().map(|f| (f.path.clone(), f.clone())).collect();

    #[derive(Clone)]
    struct TreeNode {
        path: String,
        children: HashMap<String, TreeNode>,
    }

    let mut root = TreeNode {
        path: cwd_str.clone(),
        children: HashMap::new(),
    };

    for file in file_map.values() {
        let parts: Vec<&str> = file.path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut root;

        for part in parts.iter() {
            let child_path = if current.path == cwd_str {
                format!("{}/{}", cwd_str, part)
            } else {
                format!("{}/{}", current.path, part)
            };

            if !current.children.contains_key(*part) {
                current.children.insert(
                    part.to_string(),
                    TreeNode {
                        path: child_path,
                        children: HashMap::new(),
                    },
                );
            }

            current = current.children.get_mut(*part).unwrap();
        }
    }

    fn build_policy_tree(
        node: &TreeNode,
        file_map: &std::collections::BTreeMap<String, FileAccess>,
    ) -> Vec<PolicyNode> {
        let mut result = Vec::new();

        if node.children.is_empty() {
            let access = file_map
                .get(&node.path)
                .map(|fa| fa.access.clone())
                .unwrap_or(Access::Deny);
            return vec![PolicyNode {
                path: node.path.clone(),
                access,
                children: vec![],
            }];
        }

        let children: Vec<PolicyNode> = node
            .children
            .values()
            .flat_map(|c| build_policy_tree(c, file_map))
            .collect();

        if !children.is_empty() {
            result.push(PolicyNode {
                path: node.path.clone(),
                access: Access::Deny,
                children,
            });
        }

        result
    }

    let entries = build_policy_tree(&root, &file_map);

    PolicyTree { entries }
}

fn trace_command(binary: &str, args: &[String]) -> Result<Vec<FileAccess>> {
    let mut cmd = Command::new("strace");
    cmd.arg("-e").arg("trace=open,openat,openat2");
    cmd.arg("-f");
    cmd.arg("-o").arg("/dev/stdout");
    cmd.arg("--");
    cmd.arg(binary);
    cmd.args(args);

    let output = cmd.output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let file_accesses = parse_strace_output(&stdout);

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
