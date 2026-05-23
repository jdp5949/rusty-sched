//! Authentication + RBAC domain types.

use crate::{ApiKeyId, UserId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// User role for RBAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Full access including user management.
    Admin,
    /// Can create/edit/run jobs but not manage users.
    Operator,
    /// Read-only access.
    Viewer,
}

impl Role {
    /// String form used in DB rows and JIL `permission` attr.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Operator => "operator",
            Role::Viewer => "viewer",
        }
    }

    /// Parse from DB / JIL string. Returns None for unknown values.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(Role::Admin),
            "operator" => Some(Role::Operator),
            "viewer" => Some(Role::Viewer),
            _ => None,
        }
    }

    /// True if this role can mutate jobs / runs / calendars.
    pub fn can_write(self) -> bool {
        matches!(self, Role::Admin | Role::Operator)
    }

    /// True if this role can manage users + API keys.
    pub fn can_admin(self) -> bool {
        matches!(self, Role::Admin)
    }
}

/// A user account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// Identifier.
    pub id: UserId,
    /// Login name (unique).
    pub username: String,
    /// Role.
    pub role: Role,
    /// Whether disabled (can't log in).
    pub disabled: bool,
    /// Created.
    pub created_at: DateTime<Utc>,
}

/// An issued API key (the raw token is shown once at creation; only the hash is stored).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKey {
    /// Identifier.
    pub id: ApiKeyId,
    /// Owning user.
    pub user_id: UserId,
    /// Friendly name (e.g., "ci", "backup-agent").
    pub name: String,
    /// Created.
    pub created_at: DateTime<Utc>,
    /// Last time the key was used to authenticate.
    pub last_used_at: Option<DateTime<Utc>>,
    /// Optional expiry.
    pub expires_at: Option<DateTime<Utc>>,
    /// Whether disabled.
    pub disabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_roundtrip() {
        for r in [Role::Admin, Role::Operator, Role::Viewer] {
            assert_eq!(Role::parse(r.as_str()), Some(r));
        }
    }

    #[test]
    fn role_capability_matrix() {
        assert!(Role::Admin.can_write());
        assert!(Role::Admin.can_admin());
        assert!(Role::Operator.can_write());
        assert!(!Role::Operator.can_admin());
        assert!(!Role::Viewer.can_write());
        assert!(!Role::Viewer.can_admin());
    }
}
