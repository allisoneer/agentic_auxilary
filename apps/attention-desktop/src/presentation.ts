export type ConnectionStatus =
  | { kind: "connecting" }
  | { kind: "connected" }
  | { kind: "reconnecting"; attempt: number }
  | { kind: "gap" }
  | { kind: "closed" };

export interface WorkItem {
  id: string;
  revision: string;
  lifecycle: "open" | "completed" | "cancelled";
  dueAt?: string;
  scheduledAt?: string;
  deferUntil?: string;
}

export interface AttentionSignal {
  id: string;
  revision: string;
  sourceLifecycle: "active" | "resolved" | "expired";
  attentionState: "unread" | "acknowledged";
}

export type ReminderTarget =
  | { kind: "work_item"; workItemId: string }
  | { kind: "attention_signal"; attentionSignalId: string };

export interface ReminderFire {
  id: string;
  triggerAt: string;
  state: "scheduled" | "fired" | "acknowledged" | "snoozed";
}

export interface Reminder {
  id: string;
  revision: string;
  target: ReminderTarget;
  triggerAt: string;
  fires: ReminderFire[];
}

export type InboxEntry =
  | { kind: "work_item"; workItem: WorkItem }
  | { kind: "attention_signal"; attentionSignal: AttentionSignal }
  | {
      kind: "reminder_fire";
      reminderId: string;
      reminderRevision: string;
      target: ReminderTarget;
      fire: ReminderFire;
    };

export type InboxKey =
  | { kind: "work_item"; workItemId: string }
  | { kind: "attention_signal"; attentionSignalId: string }
  | { kind: "reminder_fire"; reminderFireId: string };

export interface SnapshotState {
  workItems: WorkItem[];
  attentionSignals: AttentionSignal[];
  reminders: Reminder[];
  inbox: InboxEntry[];
}

export type AffectedView =
  | { kind: "work_item"; workItem: WorkItem }
  | { kind: "attention_signal"; attentionSignal: AttentionSignal }
  | { kind: "reminder"; reminder: Reminder };

export interface ChangeEvent {
  id: string;
  cursor: string;
  occurredAt: string;
  kind: string;
  affected: AffectedView[];
  inbox: { upserts: InboxEntry[]; removals: InboxKey[] };
}

export type DesktopMessage =
  | { type: "status"; sequence: number; generation: number; status: ConnectionStatus }
  | {
      type: "reset";
      sequence: number;
      generation: number;
      reason: "gap" | "stream_changed" | "overflow" | "emission_failed";
    }
  | {
      type: "snapshot";
      sequence: number;
      generation: number;
      state: SnapshotState;
      afterCursor: string;
    }
  | { type: "change"; sequence: number; generation: number; event: ChangeEvent }
  | {
      type: "issue";
      sequence: number;
      generation: number;
      issue: { category: string; message: string };
    };

export interface DesktopState {
  sequence: number;
  generation: number;
  status: ConnectionStatus;
  snapshot?: SnapshotState;
  snapshotAfterCursor?: string;
  issue?: { category: string; message: string };
  replay: DesktopMessage[];
}

export type MutationKind =
  | "create_work_item"
  | "complete_work_item"
  | "cancel_work_item"
  | "acknowledge_attention_signal"
  | "create_reminder"
  | "acknowledge_reminder_fire"
  | "snooze_reminder_fire";

export interface MutationReceipt {
  disposition: "applied" | "replayed";
  cursor: string;
  changeEventId: string;
  resource:
    | { kind: "work_item"; id: string }
    | { kind: "attention_signal"; id: string }
    | { kind: "reminder_fire"; reminderId: string; fireId: string };
}

export type DesktopCommandError =
  | {
      category: "expected_revision_conflict";
      message: string;
      resourceKind: "work_item" | "attention_signal" | "reminder" | "reminder_fire";
      resourceId: string;
      expectedRevision: string;
      actualRevision: string;
    }
  | {
      category: "create_conflict";
      message: string;
      resourceKind: "work_item" | "attention_signal" | "reminder" | "reminder_fire";
      resourceId: string;
    }
  | {
      category:
        | "ambiguous_mutation"
        | "validation"
        | "transport"
        | "timeout"
        | "backpressure"
        | "closed"
        | "peer";
      message: string;
    };
