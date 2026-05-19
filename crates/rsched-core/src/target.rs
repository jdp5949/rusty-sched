//! Where a job should run.

use crate::AgentId;
use serde::{Deserialize, Serialize};

/// Target discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    /// Specific agent id.
    Specific,
    /// Any agent whose tag set contains the listed tag.
    Tag,
    /// Any healthy agent.
    Any,
}

/// Tagged target spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    /// Run on a specific agent.
    Specific {
        /// Agent id.
        agent_id: AgentId,
    },
    /// Run on any agent matching tag.
    Tag {
        /// Tag string.
        tag: String,
    },
    /// Run on any healthy agent.
    Any,
}

impl Target {
    /// Return the discriminant kind.
    pub fn kind(&self) -> TargetKind {
        match self {
            Target::Specific { .. } => TargetKind::Specific,
            Target::Tag { .. } => TargetKind::Tag,
            Target::Any => TargetKind::Any,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let t = Target::Tag { tag: "etl".into() };
        let json = serde_json::to_string(&t).unwrap();
        let back: Target = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
        assert_eq!(t.kind(), TargetKind::Tag);
    }
}
