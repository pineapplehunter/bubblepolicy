use serde::{Deserialize, Serialize};

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
pub struct PolicyNode {
    pub path: String,
    pub access: Access,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PolicyNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyTree {
    pub entries: Vec<PolicyNode>,
}

pub type Trees = Vec<PolicyTree>;

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
    fn test_access_is_tmpfs() {
        assert!(!Access::Deny.is_tmpfs());
        assert!(!Access::ReadOnly.is_tmpfs());
        assert!(!Access::ReadWrite.is_tmpfs());
        assert!(Access::Tmpfs.is_tmpfs());
    }

    #[test]
    fn test_access_is_write() {
        assert!(!Access::Deny.is_write());
        assert!(!Access::ReadOnly.is_write());
        assert!(Access::ReadWrite.is_write());
        assert!(Access::Tmpfs.is_write());
    }

    #[test]
    fn test_policy_node_serialization() {
        let node = PolicyNode {
            path: "/test".to_string(),
            access: Access::ReadOnly,
            children: vec![],
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("ReadOnly"));
        assert!(json.contains("/test"));
    }

    #[test]
    fn test_policy_node_with_children() {
        let node = PolicyNode {
            path: "/".to_string(),
            access: Access::Deny,
            children: vec![PolicyNode {
                path: "/bin".to_string(),
                access: Access::ReadOnly,
                children: vec![],
            }],
        };
        let json = serde_json::to_string_pretty(&node).unwrap();
        assert!(json.contains("/bin"));
    }

    #[test]
    fn test_policy_tree_serialization() {
        let tree = PolicyTree {
            entries: vec![PolicyNode {
                path: "/".to_string(),
                access: Access::Deny,
                children: vec![],
            }],
        };
        let json = serde_json::to_string(&tree).unwrap();
        assert!(json.contains("entries"));
    }

    #[test]
    fn test_trees_type() {
        let trees: Trees = vec![
            PolicyTree {
                entries: vec![PolicyNode {
                    path: "/".to_string(),
                    access: Access::ReadOnly,
                    children: vec![],
                }],
            },
            PolicyTree {
                entries: vec![PolicyNode {
                    path: "/home".to_string(),
                    access: Access::ReadOnly,
                    children: vec![],
                }],
            },
        ];
        assert_eq!(trees.len(), 2);
    }
}
