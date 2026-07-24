//! Immutable source provenance values.

use crate::InvariantError;
use crate::SourceEntityId;
use crate::SourceReceiptId;
use chrono::DateTime;
use chrono::Utc;

macro_rules! define_source_component {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, InvariantError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(InvariantError::EmptySourceComponent { component: $label });
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

define_source_component!(SourceKind, "source_kind");
define_source_component!(SourceInstance, "source_instance");
define_source_component!(OccurrenceId, "occurrence_id");
define_source_component!(ExternalEntityId, "external_entity_id");

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OccurrenceKey {
    source_kind: SourceKind,
    source_instance: SourceInstance,
    occurrence_id: OccurrenceId,
}

impl OccurrenceKey {
    pub const fn new(
        source_kind: SourceKind,
        source_instance: SourceInstance,
        occurrence_id: OccurrenceId,
    ) -> Self {
        Self {
            source_kind,
            source_instance,
            occurrence_id,
        }
    }

    pub const fn source_kind(&self) -> &SourceKind {
        &self.source_kind
    }

    pub const fn source_instance(&self) -> &SourceInstance {
        &self.source_instance
    }

    pub const fn occurrence_id(&self) -> &OccurrenceId {
        &self.occurrence_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceEntityKey {
    source_kind: SourceKind,
    source_instance: SourceInstance,
    external_entity_id: ExternalEntityId,
}

impl SourceEntityKey {
    pub const fn new(
        source_kind: SourceKind,
        source_instance: SourceInstance,
        external_entity_id: ExternalEntityId,
    ) -> Self {
        Self {
            source_kind,
            source_instance,
            external_entity_id,
        }
    }

    pub const fn source_kind(&self) -> &SourceKind {
        &self.source_kind
    }

    pub const fn source_instance(&self) -> &SourceInstance {
        &self.source_instance
    }

    pub const fn external_entity_id(&self) -> &ExternalEntityId {
        &self.external_entity_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceReceipt {
    id: SourceReceiptId,
    occurrence_key: OccurrenceKey,
    source_entity_key: Option<SourceEntityKey>,
    occurred_at: DateTime<Utc>,
    ingested_at: DateTime<Utc>,
}

impl SourceReceipt {
    pub fn reconstruct(
        id: SourceReceiptId,
        occurrence_key: OccurrenceKey,
        source_entity_key: Option<SourceEntityKey>,
        occurred_at: DateTime<Utc>,
        ingested_at: DateTime<Utc>,
    ) -> Result<Self, InvariantError> {
        Ok(Self {
            id,
            occurrence_key,
            source_entity_key,
            occurred_at,
            ingested_at,
        })
    }

    pub const fn id(&self) -> SourceReceiptId {
        self.id
    }

    pub const fn occurrence_key(&self) -> &OccurrenceKey {
        &self.occurrence_key
    }

    pub const fn source_entity_key(&self) -> Option<&SourceEntityKey> {
        self.source_entity_key.as_ref()
    }

    pub const fn occurred_at(&self) -> &DateTime<Utc> {
        &self.occurred_at
    }

    pub const fn ingested_at(&self) -> &DateTime<Utc> {
        &self.ingested_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEntity {
    id: SourceEntityId,
    key: SourceEntityKey,
}

impl SourceEntity {
    pub const fn reconstruct(
        id: SourceEntityId,
        key: SourceEntityKey,
    ) -> Result<Self, InvariantError> {
        Ok(Self { id, key })
    }

    pub const fn id(&self) -> SourceEntityId {
        self.id
    }

    pub const fn key(&self) -> &SourceEntityKey {
        &self.key
    }
}
