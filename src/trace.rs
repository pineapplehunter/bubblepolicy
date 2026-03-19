use color_eyre::{Result, eyre::bail};
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use crate::common::{Access, PolicyNode, PolicyTree};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileAccess {
    pub path: String,
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

    // Split into absolute and relative paths
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
                .map(|_fa| Access::Deny)
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
                .map(|_fa| Access::Deny)
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
    let mut file_map: BTreeMap<String, FileAccess> = BTreeMap::new();

    // Build the command: strace -e trace=open,openat,openat2 -f -o /dev/stdout -- <binary> <args>
    let mut cmd = Command::new("strace");
    cmd.arg("-e").arg("trace=open,openat,openat2");
    cmd.arg("-f");
    cmd.arg("-o").arg("/dev/stdout");
    cmd.arg("--");
    cmd.arg(binary);
    cmd.args(args);

    let output = cmd.output()?;

    // Parse strace output - it goes to stdout when using -o /dev/stdout
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // strace output format: <pid> openat(AT_FDCWD, "/path/to/file", O_RDONLY) = 3
        // or: <pid> open("/path/to/file", O_RDONLY) = 3
        if let Some(path) = parse_strace_line(line) {
            if let Some(p) = filter_path(&path) {
                file_map.entry(p.clone()).or_insert(FileAccess { path: p });
            }
        }
    }

    let mut files: Vec<FileAccess> = file_map.into_values().collect();
    files.sort();

    Ok(files)
}

fn parse_strace_line(line: &str) -> Option<String> {
    // Match openat(AT_FDCWD, "/path", ...) or open("/path", ...)
    if let Some(at_fdcwd_pos) = line.find("AT_FDCWD") {
        // openat format: openat(AT_FDCWD, "/path", ...)
        if let Some(quote_start) = line[at_fdcwd_pos..].find('"') {
            let rest = &line[at_fdcwd_pos + quote_start + 1..];
            if let Some(quote_end) = rest.find('"') {
                let path = &rest[..quote_end];
                return Some(path.to_string());
            }
        }
    } else if let Some(open_pos) = line.find("open(") {
        // open format: open("/path", ...)
        if let Some(quote_start) = line[open_pos..].find('"') {
            let rest = &line[open_pos + quote_start + 1..];
            if let Some(quote_end) = rest.find('"') {
                let path = &rest[..quote_end];
                return Some(path.to_string());
            }
        }
    } else if let Some(openat_pos) = line.find("openat(") {
        // openat format without AT_FDCWD: openat("/path", ...)
        if let Some(quote_start) = line[openat_pos..].find('"') {
            let rest = &line[openat_pos + quote_start + 1..];
            if let Some(quote_end) = rest.find('"') {
                let path = &rest[..quote_end];
                return Some(path.to_string());
            }
        }
    }
    None
}

fn filter_path(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_path_absolute() {
        let result = filter_path("/absolute/path");
        assert_eq!(result, Some("/absolute/path".to_string()));
    }

    #[test]
    fn test_filter_path_empty() {
        let result = filter_path("");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_strace_openat() {
        let line = "12345 openat(AT_FDCWD, \"/etc/passwd\", O_RDONLY) = 3";
        assert_eq!(parse_strace_line(line), Some("/etc/passwd".to_string()));
    }

    #[test]
    fn test_parse_strace_open() {
        let line = "12345 open(\"/etc/passwd\", O_RDONLY) = 3";
        assert_eq!(parse_strace_line(line), Some("/etc/passwd".to_string()));
    }
}
