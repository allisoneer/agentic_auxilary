//! Immutable source provenance values.

use crate::CanonicalFingerprint;
use crate::InvariantError;
use crate::ObservedSourceAuthority;
use crate::SourceEntityId;
use crate::SourceOrderMode;
use crate::SourceReceiptId;
use crate::SourceStateVersion;
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
    fingerprint: CanonicalFingerprint,
    source_order: SourceOrderMode,
    occurred_at: DateTime<Utc>,
    ingested_at: DateTime<Utc>,
}

impl SourceReceipt {
    pub fn reconstruct(
        id: SourceReceiptId,
        occurrence_key: OccurrenceKey,
        source_entity_key: Option<SourceEntityKey>,
        fingerprint: CanonicalFingerprint,
        source_order: SourceOrderMode,
        occurred_at: DateTime<Utc>,
        ingested_at: DateTime<Utc>,
    ) -> Result<Self, InvariantError> {
        Ok(Self {
            id,
            occurrence_key,
            source_entity_key,
            fingerprint,
            source_order,
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

    pub const fn fingerprint(&self) -> CanonicalFingerprint {
        self.fingerprint
    }

    pub const fn source_order(&self) -> &SourceOrderMode {
        &self.source_order
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
    version: SourceStateVersion,
    latest_receipt_id: SourceReceiptId,
    order: SourceOrderMode,
}

impl SourceEntity {
    pub const fn reconstruct(
        id: SourceEntityId,
        key: SourceEntityKey,
        version: SourceStateVersion,
        latest_receipt_id: SourceReceiptId,
        order: SourceOrderMode,
    ) -> Result<Self, InvariantError> {
        Ok(Self {
            id,
            key,
            version,
            latest_receipt_id,
            order,
        })
    }

    pub const fn id(&self) -> SourceEntityId {
        self.id
    }

    pub const fn key(&self) -> &SourceEntityKey {
        &self.key
    }

    pub const fn version(&self) -> SourceStateVersion {
        self.version
    }

    pub const fn latest_receipt_id(&self) -> SourceReceiptId {
        self.latest_receipt_id
    }

    pub const fn order(&self) -> &SourceOrderMode {
        &self.order
    }

    pub fn advance(
        &self,
        latest_receipt_id: SourceReceiptId,
        order: SourceOrderMode,
    ) -> Result<Self, InvariantError> {
        Ok(Self {
            id: self.id,
            key: self.key.clone(),
            version: self.version.checked_increment()?,
            latest_receipt_id,
            order,
        })
    }

    pub fn observed_authority(&self) -> ObservedSourceAuthority {
        ObservedSourceAuthority::Present {
            version: self.version,
            latest_receipt_id: self.latest_receipt_id,
            order: self.order.clone(),
        }
    }
}
