//! Virtual resources — named counters with fixed capacity.
//!
//! Jobs declare `ResourceClaim { resource_name, units }`. Before dispatch
//! the scheduler attempts to acquire every claim atomically; if any claim
//! exceeds remaining capacity the job is left queued. On run finish the
//! scheduler releases the holds.
//!
//! Resources are referenced by **name** in `Job.resource_claims` so JIL +
//! the REST API are name-friendly; the store resolves to `ResourceId`.

use crate::{CoreError, ResourceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A named virtual resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    /// Identifier.
    pub id: ResourceId,
    /// Display name (unique).
    pub name: String,
    /// Maximum units that can be held at once.
    pub capacity: u32,
    /// Optional human description.
    #[serde(default)]
    pub description: Option<String>,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
}

impl Resource {
    /// Validate.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.name.trim().is_empty() || self.name.len() > 200 {
            return Err(CoreError::InvalidName(self.name.clone(), "len 1..=200"));
        }
        if !self
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return Err(CoreError::InvalidName(
                self.name.clone(),
                "only [A-Za-z0-9_.-] allowed",
            ));
        }
        Ok(())
    }
}

/// A job's declared claim on a resource. Resolved by name at acquire time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceClaim {
    /// Resource name (looked up in `resources.name`).
    pub resource_name: String,
    /// Units required to start (must be > 0).
    pub units: u32,
}

impl ResourceClaim {
    /// Validate.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.resource_name.trim().is_empty() {
            return Err(CoreError::InvalidName(
                self.resource_name.clone(),
                "resource_name required",
            ));
        }
        if self.units == 0 {
            return Err(CoreError::InvalidRetry("resource units must be > 0"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_resource(name: &str, capacity: u32) -> Resource {
        Resource {
            id: ResourceId::new(),
            name: name.into(),
            capacity,
            description: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn validate_ok() {
        assert!(make_resource("db.connections", 10).validate().is_ok());
    }

    #[test]
    fn validate_bad_name() {
        assert!(make_resource("", 1).validate().is_err());
        assert!(make_resource("bad name!", 1).validate().is_err());
    }

    #[test]
    fn claim_zero_units_rejected() {
        let c = ResourceClaim {
            resource_name: "db".into(),
            units: 0,
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn claim_empty_name_rejected() {
        let c = ResourceClaim {
            resource_name: "".into(),
            units: 1,
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let r = make_resource("db.connections", 25);
        let s = serde_json::to_string(&r).unwrap();
        let back: Resource = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn claim_serde_roundtrip() {
        let c = ResourceClaim {
            resource_name: "db".into(),
            units: 3,
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: ResourceClaim = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }
}
