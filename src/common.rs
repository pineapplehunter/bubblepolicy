use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
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

    pub fn to_str(&self) -> &'static str {
        match self {
            Access::Deny => "Deny",
            Access::ReadOnly => "ReadOnly",
            Access::ReadWrite => "ReadWrite",
            Access::Tmpfs => "Tmpfs",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Deny" => Some(Access::Deny),
            "ReadOnly" => Some(Access::ReadOnly),
            "ReadWrite" => Some(Access::ReadWrite),
            "Tmpfs" => Some(Access::Tmpfs),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PolicyEntry {
    pub path: String,
    pub access: Access,
}

pub fn parse_entries(content: &str) -> Vec<PolicyEntry> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let parts: Vec<&str> = line.splitn(2, ' ').collect();
            if parts.len() != 2 {
                return None;
            }
            let access = Access::parse(parts[0])?;
            let path = parts[1].to_string();
            Some(PolicyEntry { path, access })
        })
        .collect()
}

pub fn entries_to_string(entries: &[PolicyEntry]) -> String {
    entries
        .iter()
        .map(|e| format!("{} {}", e.access.to_str(), e.path))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn dedup_entries(entries: &[PolicyEntry]) -> Vec<PolicyEntry> {
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

    for entry in entries {
        if !entry.access.is_allowed() {
            continue;
        }

        let path = &entry.path;

        if path == "/" {
            root.access = Some(entry.access);
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

            current = current.children.get_mut(*part).unwrap();
            if is_last {
                current.access = Some(entry.access);
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
            if let Some(access) = &node.access {
                let diff = match parent_access {
                    None => true,
                    Some(p) => !std::ptr::eq(p, access),
                };
                if diff {
                    result.push(PolicyEntry {
                        path: node.path.clone(),
                        access: *access,
                    });
                }
            } else {
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
                    Some(p) => !std::ptr::eq(p, access),
                };
                if diff {
                    result.push(PolicyEntry {
                        path: node.path.clone(),
                        access: *access,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_is_allowed() {
        assert!(!Access::Deny.is_allowed());
        assert!(Access::ReadOnly.is_allowed());
        assert!(Access::ReadWrite.is_allowed());
        assert!(Access::Tmpfs.is_allowed());
    }

    #[test]
    fn test_access_to_str() {
        assert_eq!(Access::Deny.to_str(), "Deny");
        assert_eq!(Access::ReadOnly.to_str(), "ReadOnly");
        assert_eq!(Access::ReadWrite.to_str(), "ReadWrite");
        assert_eq!(Access::Tmpfs.to_str(), "Tmpfs");
    }

    #[test]
    fn test_access_parse() {
        assert_eq!(Access::parse("Deny"), Some(Access::Deny));
        assert_eq!(Access::parse("ReadOnly"), Some(Access::ReadOnly));
        assert_eq!(Access::parse("ReadWrite"), Some(Access::ReadWrite));
        assert_eq!(Access::parse("Tmpfs"), Some(Access::Tmpfs));
        assert_eq!(Access::parse("Invalid"), None);
    }

    #[test]
    fn test_parse_entries() {
        let content = "ReadOnly /etc/passwd\nReadWrite /tmp/file";
        let entries = parse_entries(content);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "/etc/passwd");
        assert_eq!(entries[0].access, Access::ReadOnly);
        assert_eq!(entries[1].path, "/tmp/file");
        assert_eq!(entries[1].access, Access::ReadWrite);
    }

    #[test]
    fn test_entries_to_string() {
        let entries = vec![
            PolicyEntry {
                path: "/etc/passwd".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/tmp/file".to_string(),
                access: Access::ReadWrite,
            },
        ];
        let result = entries_to_string(&entries);
        assert_eq!(result, "ReadOnly /etc/passwd\nReadWrite /tmp/file");
    }

    #[test]
    fn test_dedup_collapse() {
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

    #[test]
    fn test_dedup_empty() {
        let entries: Vec<PolicyEntry> = vec![];
        let result = dedup_entries(&entries);
        assert!(result.is_empty());
    }

    #[test]
    fn test_dedup_multiple_roots() {
        let entries = vec![
            PolicyEntry {
                path: "/a".to_string(),
                access: Access::ReadOnly,
            },
            PolicyEntry {
                path: "/b".to_string(),
                access: Access::ReadOnly,
            },
        ];
        let result = dedup_entries(&entries);
        assert_eq!(result.len(), 2);
    }
}
