//! Strongly-typed ULID-based IDs to prevent mixing different resource ids.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use ulid::Ulid;

macro_rules! define_id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Ulid);

        impl $name {
            /// Generate a new random ID.
            pub fn new() -> Self {
                Self(Ulid::new())
            }
            /// Underlying ULID.
            pub fn ulid(&self) -> Ulid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = ulid::DecodeError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Ulid::from_string(s)?))
            }
        }
    };
}

define_id!(/// Identifier for a [`Job`](crate::Job).
    JobId);
define_id!(/// Identifier for a [`Run`](crate::Run).
    RunId);
define_id!(/// Identifier for a job [`Box`](crate::JobBox).
    BoxId);
define_id!(/// Identifier for a [`Calendar`](crate::Calendar).
    CalendarId);
define_id!(/// Identifier for an agent.
    AgentId);
define_id!(/// Identifier for a user.
    UserId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_string() {
        let id = JobId::new();
        let s = id.to_string();
        let parsed: JobId = s.parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn distinct_ids_are_unequal() {
        assert_ne!(JobId::new(), JobId::new());
    }

    #[test]
    fn json_roundtrip() {
        let id = AgentId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: AgentId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
