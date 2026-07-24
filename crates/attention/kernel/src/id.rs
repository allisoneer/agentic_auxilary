//! Distinct native durable identities.

use crate::InvariantError;
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;
use uuid::Version;

macro_rules! define_uuid_v7_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = InvariantError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                if value.get_version() != Some(Version::SortRand) {
                    return Err(InvariantError::InvalidUuidVersion);
                }
                Ok(Self(value))
            }
        }

        impl FromStr for $name {
            type Err = InvariantError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let parsed = Uuid::parse_str(value)
                    .map_err(|_| InvariantError::InvalidUuidText(value.to_string()))?;
                if parsed.hyphenated().to_string() != value {
                    return Err(InvariantError::NonCanonicalUuidText(value.to_string()));
                }
                Self::try_from(parsed)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

define_uuid_v7_id!(WorkItemId);
define_uuid_v7_id!(AttentionSignalId);
define_uuid_v7_id!(ReminderId);
define_uuid_v7_id!(ReminderFireId);
define_uuid_v7_id!(SourceReceiptId);
define_uuid_v7_id!(SourceEntityId);
define_uuid_v7_id!(ChangeEventId);
define_uuid_v7_id!(OutboxIntentId);
