//! Typed known-operation catalog layered over open JSON-RPC envelopes.

use crate::AcknowledgeAttentionSignalParams;
use crate::AcknowledgeAttentionSignalResult;
use crate::AcknowledgeReminderFireParams;
use crate::AcknowledgeReminderFireResult;
use crate::AttentionSignalId;
use crate::AttentionSignalView;
use crate::CancelWorkItemParams;
use crate::CancelWorkItemResult;
use crate::ChangeEvent;
use crate::ChangesResult;
use crate::CompleteWorkItemParams;
use crate::CompleteWorkItemResult;
use crate::CreateReminderParams;
use crate::CreateReminderResult;
use crate::CreateWorkItemParams;
use crate::CreateWorkItemResult;
use crate::Cursor;
use crate::DeliveryClaimParams;
use crate::DeliveryClaimResult;
use crate::DeliveryFailRetryableParams;
use crate::DeliveryFailRetryableResult;
use crate::DeliveryFailTerminalParams;
use crate::DeliveryFailTerminalResult;
use crate::DeliveryInspectParams;
use crate::DeliveryInspectResult;
use crate::DeliveryRenewParams;
use crate::DeliveryRenewResult;
use crate::DeliverySucceedParams;
use crate::DeliverySucceedResult;
use crate::IngestSourceOccurrenceParams;
use crate::IngestSourceOccurrenceResult;
use crate::JsonRpcVersion;
use crate::ReminderId;
use crate::ReminderView;
use crate::RequestId;
use crate::RpcNotification;
use crate::RpcRequest;
use crate::SnapshotResult;
use crate::SnoozeReminderFireParams;
use crate::SnoozeReminderFireResult;
use crate::SourceEntityKey;
use crate::SourceEntityView;
use crate::SourceReceiptId;
use crate::SourceReceiptView;
use crate::WorkItemId;
use crate::WorkItemView;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;

mod private {
    pub trait Sealed {}
}

/// A sealed known JSON-RPC request method with fixed params and result types.
pub trait RpcMethod: private::Sealed {
    type Params: Serialize + DeserializeOwned;
    type Result: Serialize + DeserializeOwned;

    const NAME: &'static str;

    /// Constructs a typed request using the catalog-owned method name.
    fn request(id: RequestId, params: Self::Params) -> RpcRequest<Self::Params> {
        RpcRequest {
            jsonrpc: JsonRpcVersion,
            id,
            method: Self::NAME.to_string(),
            params: Some(params),
        }
    }
}

/// A sealed known JSON-RPC notification method with fixed params.
pub trait RpcNotificationMethod: private::Sealed {
    type Params: Serialize + DeserializeOwned;

    const NAME: &'static str;

    /// Constructs a typed notification using the catalog-owned method name.
    fn notification(params: Self::Params) -> RpcNotification<Self::Params> {
        RpcNotification {
            jsonrpc: JsonRpcVersion,
            method: Self::NAME.to_string(),
            params: Some(params),
        }
    }
}

/// Required object-shaped parameters for known methods with no inputs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmptyParams {}

macro_rules! id_params {
    ($(#[$meta:meta])* $name:ident, $field:ident, $type:ty) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name {
            pub $field: $type,
        }
    };
}

id_params!(WorkItemGetParams, id, WorkItemId);
id_params!(AttentionSignalGetParams, id, AttentionSignalId);
id_params!(ReminderGetParams, id, ReminderId);
id_params!(SourceReceiptGetParams, id, SourceReceiptId);

/// Parameters for reading source authority by its stable key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEntityGetParams {
    pub key: SourceEntityKey,
}

/// Parameters for reading retained changes after a cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangesGetParams {
    pub after_cursor: Cursor,
    pub limit: u32,
}

/// Parameters carried by the `attention.change` notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeNotificationParams {
    pub event: ChangeEvent,
}

macro_rules! declare_catalog {
    (
        methods { $(
            $(#[$method_meta:meta])*
            $marker:ident => $name:literal ($params:ty) -> $result:ty;
        )* }
        notifications { $(
            $(#[$notification_meta:meta])*
            $notification_marker:ident => $notification_name:literal ($notification_params:ty);
        )* }
    ) => {
        $(
            $(#[$method_meta])*
            #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
            pub struct $marker;

            impl private::Sealed for $marker {}

            impl RpcMethod for $marker {
                type Params = $params;
                type Result = $result;

                const NAME: &'static str = $name;
            }
        )*

        $(
            $(#[$notification_meta])*
            #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
            pub struct $notification_marker;

            impl private::Sealed for $notification_marker {}

            impl RpcNotificationMethod for $notification_marker {
                type Params = $notification_params;

                const NAME: &'static str = $notification_name;
            }
        )*

        /// Exact known v1 request method names, used by conformance governance.
        pub const V1_METHOD_NAMES: &[&str] = &[$($name),*];

        /// Exact known v1 notification method names, used by conformance governance.
        pub const V1_NOTIFICATION_METHOD_NAMES: &[&str] = &[$($notification_name),*];
    };
}

declare_catalog! {
    methods {
        WorkItemGet => "attention.work_item.get" (WorkItemGetParams) -> WorkItemView;
        AttentionSignalGet => "attention.signal.get" (AttentionSignalGetParams) -> AttentionSignalView;
        ReminderGet => "attention.reminder.get" (ReminderGetParams) -> ReminderView;
        SourceEntityGet => "attention.source_entity.get" (SourceEntityGetParams) -> SourceEntityView;
        SourceReceiptGet => "attention.source_receipt.get" (SourceReceiptGetParams) -> SourceReceiptView;
        SnapshotGet => "attention.snapshot.get" (EmptyParams) -> SnapshotResult;
        ChangesGet => "attention.changes.get" (ChangesGetParams) -> ChangesResult;
        WorkItemCreate => "attention.work_item.create" (CreateWorkItemParams) -> CreateWorkItemResult;
        WorkItemComplete => "attention.work_item.complete" (CompleteWorkItemParams) -> CompleteWorkItemResult;
        WorkItemCancel => "attention.work_item.cancel" (CancelWorkItemParams) -> CancelWorkItemResult;
        AttentionSignalAcknowledge => "attention.signal.acknowledge" (AcknowledgeAttentionSignalParams) -> AcknowledgeAttentionSignalResult;
        SourceOccurrenceIngest => "attention.source_occurrence.ingest" (IngestSourceOccurrenceParams) -> IngestSourceOccurrenceResult;
        ReminderCreate => "attention.reminder.create" (CreateReminderParams) -> CreateReminderResult;
        ReminderFireAcknowledge => "attention.reminder_fire.acknowledge" (AcknowledgeReminderFireParams) -> AcknowledgeReminderFireResult;
        ReminderFireSnooze => "attention.reminder_fire.snooze" (SnoozeReminderFireParams) -> SnoozeReminderFireResult;
        DeliveryClaim => "attention.delivery.claim" (DeliveryClaimParams) -> DeliveryClaimResult;
        DeliveryInspect => "attention.delivery.inspect" (DeliveryInspectParams) -> DeliveryInspectResult;
        DeliveryRenew => "attention.delivery.renew" (DeliveryRenewParams) -> DeliveryRenewResult;
        DeliverySucceed => "attention.delivery.succeed" (DeliverySucceedParams) -> DeliverySucceedResult;
        DeliveryFailRetryable => "attention.delivery.fail_retryable" (DeliveryFailRetryableParams) -> DeliveryFailRetryableResult;
        DeliveryFailTerminal => "attention.delivery.fail_terminal" (DeliveryFailTerminalParams) -> DeliveryFailTerminalResult;
    }
    notifications {
        AttentionChange => "attention.change" (ChangeNotificationParams);
    }
}
