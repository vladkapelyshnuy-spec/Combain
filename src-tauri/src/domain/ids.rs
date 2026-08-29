//! Typed identifiers for domain entities.
//!
//! Each ID wraps a UUID so entities cannot be mixed up at compile time
//! (e.g. passing a `SceneId` where a `ProjectId` is expected).

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generate a new random identifier.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wrap an existing UUID (e.g. loaded from storage).
            pub fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }

            /// Access the underlying UUID.
            pub fn as_uuid(&self) -> Uuid {
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
                write!(f, "{}", self.0)
            }
        }
    };
}

typed_id!(ProjectId);
typed_id!(SceneId);
typed_id!(AssetId);
typed_id!(AssetVersionId);
typed_id!(TimelineId);
typed_id!(TimelineClipId);
typed_id!(JobId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_unique_ids() {
        assert_ne!(ProjectId::new(), ProjectId::new());
    }

    #[test]
    fn serializes_as_plain_uuid_string() {
        let id = SceneId::from_uuid(Uuid::nil());
        let json = serde_json::to_string(&id).expect("serialize");
        assert_eq!(json, "\"00000000-0000-0000-0000-000000000000\"");
    }

    #[test]
    fn round_trips_through_json() {
        let id = AssetId::new();
        let json = serde_json::to_string(&id).expect("serialize");
        let back: AssetId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }
}
