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
