//! Provider-neutral delivery authority contracts.

use crate::CommitCursor;
use crate::InvariantError;
use crate::OutboxIntentId;
use chrono::DateTime;
use chrono::Utc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeliveryLeaseToken([u8; 32]);

impl DeliveryLeaseToken {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedDeliveryText(String);

impl BoundedDeliveryText {
    pub fn new(value: impl Into<String>, maximum_length: usize) -> Result<Self, InvariantError> {
        let value = value.into();
        if maximum_length == 0 {
            return Err(InvariantError::ZeroBound {
                value: "delivery text maximum length",
            });
        }
        if value.len() > maximum_length {
            return Err(InvariantError::BoundExceeded {
                value: "delivery text",
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMessageId(BoundedDeliveryText);

impl ProviderMessageId {
    pub fn new(value: impl Into<String>, maximum_length: usize) -> Result<Self, InvariantError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(InvariantError::EmptyValue {
                value: "provider message ID",
            });
        }
        Ok(Self(BoundedDeliveryText::new(value, maximum_length)?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryStatus {
    Pending,
    Leased {
        token: DeliveryLeaseToken,
        expires_at: DateTime<Utc>,
    },
    Retryable {
        attempt: u32,
        error: BoundedDeliveryText,
        next_retry_at: DateTime<Utc>,
    },
    Succeeded {
        provider_message_id: ProviderMessageId,
        succeeded_at: DateTime<Utc>,
    },
    Skipped {
        reason: BoundedDeliveryText,
        skipped_at: DateTime<Utc>,
    },
    TerminalFailure {
        attempt: u32,
        error: BoundedDeliveryText,
        failed_at: DateTime<Utc>,
    },
}

impl DeliveryStatus {
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded { .. } | Self::Skipped { .. } | Self::TerminalFailure { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryState {
    intent_id: OutboxIntentId,
    status: DeliveryStatus,
}

impl DeliveryState {
    pub const fn pending(intent_id: OutboxIntentId) -> Self {
        Self {
            intent_id,
            status: DeliveryStatus::Pending,
        }
    }

    pub const fn reconstruct(intent_id: OutboxIntentId, status: DeliveryStatus) -> Self {
        Self { intent_id, status }
    }

    pub const fn intent_id(&self) -> OutboxIntentId {
        self.intent_id
    }

    pub const fn status(&self) -> &DeliveryStatus {
        &self.status
    }

    /// Transitions an eligible delivery into a fresh lease.
    ///
    /// Callers must invoke this only for `Pending` deliveries or for `Leased` and `Retryable`
    /// deliveries whose expiration or retry time has elapsed. This method performs no eligibility
    /// time check and overwrites any non-terminal status.
    pub fn claim(&mut self, token: DeliveryLeaseToken, expires_at: DateTime<Utc>) -> ClaimOutcome {
        if self.status.is_terminal() {
            return ClaimOutcome::Terminal;
        }
        self.status = DeliveryStatus::Leased { token, expires_at };
        ClaimOutcome::Claimed(DeliveryClaim {
            intent_id: self.intent_id,
            token,
            expires_at,
        })
    }

    pub fn renew(&mut self, token: DeliveryLeaseToken, expires_at: DateTime<Utc>) -> RenewOutcome {
        let DeliveryStatus::Leased { token: current, .. } = &mut self.status else {
            return RenewOutcome::NotLeased;
        };
        if *current != token {
            return RenewOutcome::Fenced;
        }
        self.status = DeliveryStatus::Leased { token, expires_at };
        RenewOutcome::Renewed
    }

    pub fn succeed(
        &mut self,
        token: DeliveryLeaseToken,
        provider_message_id: ProviderMessageId,
        succeeded_at: DateTime<Utc>,
    ) -> DeliveryCompletionOutcome {
        if let DeliveryStatus::Succeeded {
            provider_message_id: current,
            succeeded_at: current_at,
        } = &self.status
        {
            return if current == &provider_message_id && current_at == &succeeded_at {
                DeliveryCompletionOutcome::Repeated
            } else {
                DeliveryCompletionOutcome::Conflict
            };
        }
        if !self.matches_lease(token) {
            return DeliveryCompletionOutcome::Fenced;
        }
        self.status = DeliveryStatus::Succeeded {
            provider_message_id,
            succeeded_at,
        };
        DeliveryCompletionOutcome::Applied
    }

    pub fn fail_retryable(
        &mut self,
        token: DeliveryLeaseToken,
        attempt: u32,
        error: BoundedDeliveryText,
        next_retry_at: DateTime<Utc>,
    ) -> DeliveryCompletionOutcome {
        if !self.matches_lease(token) {
            return DeliveryCompletionOutcome::Fenced;
        }
        self.status = DeliveryStatus::Retryable {
            attempt,
            error,
            next_retry_at,
        };
        DeliveryCompletionOutcome::Applied
    }

    pub fn fail_terminal(
        &mut self,
        token: DeliveryLeaseToken,
        attempt: u32,
        error: BoundedDeliveryText,
        failed_at: DateTime<Utc>,
    ) -> DeliveryCompletionOutcome {
        if let DeliveryStatus::TerminalFailure {
            attempt: current_attempt,
            error: current_error,
            failed_at: current_at,
        } = &self.status
        {
            return if *current_attempt == attempt
                && current_error == &error
                && current_at == &failed_at
            {
                DeliveryCompletionOutcome::Repeated
            } else {
                DeliveryCompletionOutcome::Conflict
            };
        }
        if !self.matches_lease(token) {
            return DeliveryCompletionOutcome::Fenced;
        }
        self.status = DeliveryStatus::TerminalFailure {
            attempt,
            error,
            failed_at,
        };
        DeliveryCompletionOutcome::Applied
    }

    pub fn skip(
        &mut self,
        token: DeliveryLeaseToken,
        reason: BoundedDeliveryText,
        skipped_at: DateTime<Utc>,
    ) -> DeliveryCompletionOutcome {
        if let DeliveryStatus::Skipped {
            reason: current_reason,
            skipped_at: current_at,
        } = &self.status
        {
            return if current_reason == &reason && current_at == &skipped_at {
                DeliveryCompletionOutcome::Repeated
            } else {
                DeliveryCompletionOutcome::Conflict
            };
        }
        if !self.matches_lease(token) {
            return DeliveryCompletionOutcome::Fenced;
        }
        self.status = DeliveryStatus::Skipped { reason, skipped_at };
        DeliveryCompletionOutcome::Applied
    }

    fn matches_lease(&self, token: DeliveryLeaseToken) -> bool {
        matches!(
            self.status,
            DeliveryStatus::Leased { token: current, .. } if current == token
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeliveryClaim {
    intent_id: OutboxIntentId,
    token: DeliveryLeaseToken,
    expires_at: DateTime<Utc>,
}

impl DeliveryClaim {
    pub const fn intent_id(self) -> OutboxIntentId {
        self.intent_id
    }

    pub const fn token(self) -> DeliveryLeaseToken {
        self.token
    }

    pub const fn expires_at(&self) -> &DateTime<Utc> {
        &self.expires_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimOutcome {
    Claimed(DeliveryClaim),
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenewOutcome {
    Renewed,
    Fenced,
    NotLeased,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryCompletionOutcome {
    Applied,
    Repeated,
    Fenced,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryCheckpoint {
    worker: BoundedDeliveryText,
    cursor: CommitCursor,
}

impl DeliveryCheckpoint {
    pub const fn new(worker: BoundedDeliveryText, cursor: CommitCursor) -> Self {
        Self { worker, cursor }
    }

    pub const fn worker(&self) -> &BoundedDeliveryText {
        &self.worker
    }

    pub const fn cursor(&self) -> CommitCursor {
        self.cursor
    }
}

#[cfg(test)]
mod tests {
    use super::DeliveryLeaseToken;

    #[test]
    fn lease_tokens_have_opaque_value_equality() {
        assert_eq!(
            DeliveryLeaseToken::from_bytes([7; 32]),
            DeliveryLeaseToken::from_bytes([7; 32])
        );
        assert_ne!(
            DeliveryLeaseToken::from_bytes([7; 32]),
            DeliveryLeaseToken::from_bytes([8; 32])
        );
    }
}
