//! Generic external delivery-worker protocol contracts.

use crate::AttentionSignalId;
use crate::ChangeEventId;
use crate::DeliveryLeaseToken;
use crate::OutboxIntentId;
use crate::ProviderMessageId;
use crate::ReminderFireId;
use crate::WireTimestamp;
use serde::Deserialize;
use serde::Serialize;

/// Frozen worker state and outcome names governed by v1 fixtures.
pub const V1_WORKER_VARIANT_NAMES: &[&str] = &[
    "state:pending",
    "state:leased",
    "state:retryable",
    "state:succeeded",
    "state:skipped",
    "state:terminal_failure",
    "renewal:renewed",
    "renewal:fenced",
    "renewal:not_leased",
    "completion:applied",
    "completion:repeated",
    "completion:fenced",
    "completion:conflict",
];

/// Provider-neutral reason an Outbox intent exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryPurpose {
    FreshAttention,
    ReminderFired,
}

/// Public resource that a delivery concerns.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeliverySubject {
    AttentionSignal {
        attention_signal_id: AttentionSignalId,
    },
    ReminderFire {
        reminder_fire_id: ReminderFireId,
    },
}

/// Bounded provider-neutral Outbox intent view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxIntentView {
    pub id: OutboxIntentId,
    pub subject: DeliverySubject,
    pub originating_change_event_id: ChangeEventId,
    pub created_at: WireTimestamp,
    pub purpose: DeliveryPurpose,
}

/// Inspectable delivery status. Leased state intentionally omits its token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DeliveryStateView {
    Pending,
    Leased {
        expires_at: WireTimestamp,
    },
    Retryable {
        attempt: u32,
        error: String,
        next_retry_at: WireTimestamp,
    },
    Succeeded {
        provider_message_id: ProviderMessageId,
        succeeded_at: WireTimestamp,
    },
    Skipped {
        reason: String,
        skipped_at: WireTimestamp,
    },
    TerminalFailure {
        attempt: u32,
        error: String,
        failed_at: WireTimestamp,
    },
}

/// Full inspectable delivery authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryAuthorityView {
    pub intent: OutboxIntentView,
    pub state: DeliveryStateView,
}

/// Bearer capability returned only by claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryClaimCapability {
    pub intent_id: OutboxIntentId,
    pub lease_token: DeliveryLeaseToken,
    pub expires_at: WireTimestamp,
}

/// Lease-renewal semantic outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryRenewalOutcome {
    Renewed,
    Fenced,
    NotLeased,
}

/// Delivery-completion semantic outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryCompletionOutcome {
    Applied,
    Repeated,
    Fenced,
    Conflict,
}

/// Parameters for atomically claiming eligible deliveries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryClaimParams {
    pub eligible_at: WireTimestamp,
    pub lease_expires_at: WireTimestamp,
    pub limit: u32,
}

/// Capabilities returned by a delivery claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryClaimResult {
    pub claims: Vec<DeliveryClaimCapability>,
}

/// Parameters for inspecting one delivery authority record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryInspectParams {
    pub intent_id: OutboxIntentId,
}

pub type DeliveryInspectResult = DeliveryAuthorityView;

/// Parameters for renewing an active delivery lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRenewParams {
    pub intent_id: OutboxIntentId,
    pub lease_token: DeliveryLeaseToken,
    pub expires_at: WireTimestamp,
}

/// Result of renewing an active delivery lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRenewResult {
    pub outcome: DeliveryRenewalOutcome,
}

/// Parameters for recording successful provider delivery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliverySucceedParams {
    pub intent_id: OutboxIntentId,
    pub lease_token: DeliveryLeaseToken,
    pub provider_message_id: ProviderMessageId,
    pub succeeded_at: WireTimestamp,
}

/// Parameters for recording a retryable provider failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryFailRetryableParams {
    pub intent_id: OutboxIntentId,
    pub lease_token: DeliveryLeaseToken,
    pub attempt: u32,
    pub error: String,
    pub next_retry_at: WireTimestamp,
}

/// Parameters for recording terminal provider failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryFailTerminalParams {
    pub intent_id: OutboxIntentId,
    pub lease_token: DeliveryLeaseToken,
    pub attempt: u32,
    pub error: String,
    pub failed_at: WireTimestamp,
}

/// Result shared by delivery completion methods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryCompletionResult {
    pub outcome: DeliveryCompletionOutcome,
}

pub type DeliverySucceedResult = DeliveryCompletionResult;
pub type DeliveryFailRetryableResult = DeliveryCompletionResult;
pub type DeliveryFailTerminalResult = DeliveryCompletionResult;
