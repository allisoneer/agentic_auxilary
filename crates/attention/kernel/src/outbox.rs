//! Provider-neutral durable delivery intents.

use crate::AttentionSignalId;
use crate::ChangeEventId;
use crate::InvariantError;
use crate::OutboxIntentId;
use crate::ReminderFireId;
use chrono::DateTime;
use chrono::Utc;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutboxDeduplicationKey(String);

impl OutboxDeduplicationKey {
    pub fn new(value: impl Into<String>, maximum_length: usize) -> Result<Self, InvariantError> {
        let value = value.into();
        if maximum_length == 0 {
            return Err(InvariantError::ZeroBound {
                value: "outbox deduplication key maximum length",
            });
        }
        if value.trim().is_empty() {
            return Err(InvariantError::EmptyValue {
                value: "outbox deduplication key",
            });
        }
        if value.trim() != value {
            return Err(InvariantError::SurroundingWhitespace {
                value: "outbox deduplication key",
            });
        }
        if value.len() > maximum_length {
            return Err(InvariantError::BoundExceeded {
                value: "outbox deduplication key",
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn for_attention_signal(id: AttentionSignalId) -> Self {
        Self(id.to_string())
    }

    pub fn for_reminder_fire(id: ReminderFireId) -> Self {
        Self(id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::AttentionSignalId;
    use super::InvariantError;
    use super::OutboxDeduplicationKey;

    #[test]
    fn deduplication_key_rejects_surrounding_whitespace_without_normalizing_identity() {
        for value in [" key", "key "] {
            assert_eq!(
                OutboxDeduplicationKey::new(value, 16),
                Err(InvariantError::SurroundingWhitespace {
                    value: "outbox deduplication key",
                })
            );
        }
        assert_eq!(
            OutboxDeduplicationKey::new(" \t\n", 16),
            Err(InvariantError::EmptyValue {
                value: "outbox deduplication key",
            })
        );

        let key = OutboxDeduplicationKey::new("signal:key", 16).expect("clean key");
        assert_eq!(key.as_str(), "signal:key");
    }

    #[test]
    fn generated_deduplication_keys_remain_stable() {
        let id = AttentionSignalId::new();
        let first = OutboxDeduplicationKey::for_attention_signal(id);
        let second = OutboxDeduplicationKey::for_attention_signal(id);
        assert_eq!(first, second);
        assert_eq!(first.as_str(), id.to_string());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeliveryPurpose {
    FreshAttention,
    ReminderFired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeliverySubject {
    AttentionSignal(AttentionSignalId),
    ReminderFire(ReminderFireId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxIntent {
    id: OutboxIntentId,
    deduplication_key: OutboxDeduplicationKey,
    subject: DeliverySubject,
    originating_change_event_id: ChangeEventId,
    created_at: DateTime<Utc>,
    purpose: DeliveryPurpose,
}

impl OutboxIntent {
    pub const fn new(
        id: OutboxIntentId,
        deduplication_key: OutboxDeduplicationKey,
        subject: DeliverySubject,
        originating_change_event_id: ChangeEventId,
        created_at: DateTime<Utc>,
        purpose: DeliveryPurpose,
    ) -> Self {
        Self {
            id,
            deduplication_key,
            subject,
            originating_change_event_id,
            created_at,
            purpose,
        }
    }

    pub const fn id(&self) -> OutboxIntentId {
        self.id
    }

    pub const fn deduplication_key(&self) -> &OutboxDeduplicationKey {
        &self.deduplication_key
    }

    pub const fn subject(&self) -> DeliverySubject {
        self.subject
    }

    pub const fn originating_change_event_id(&self) -> ChangeEventId {
        self.originating_change_event_id
    }

    pub const fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    pub const fn purpose(&self) -> DeliveryPurpose {
        self.purpose
    }
}
