use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
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
}
