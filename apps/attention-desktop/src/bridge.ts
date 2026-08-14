import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  DesktopCommandError,
  DesktopMessage,
  DesktopState,
  MutationReceipt,
  ReminderTarget,
} from "./presentation";

export interface CreateWorkItemInput {
  dueAt?: string;
  scheduledAt?: string;
  deferUntil?: string;
}
export interface ExistingWorkItemInput {
  id: string;
  expectedRevision: string;
}
export interface AcknowledgeSignalInput extends ExistingWorkItemInput {}
export interface CreateReminderInput {
  target: ReminderTarget;
  triggerAt: string;
}
export interface AcknowledgeFireInput {
  reminderId: string;
  fireId: string;
  expectedRevision: string;
}
export interface SnoozeFireInput extends AcknowledgeFireInput {
  replacementTriggerAt: string;
}

export interface DesktopBridge {
  state(): Promise<DesktopState>;
  acknowledgeSnapshot(generation: number, afterCursor: string): Promise<void>;
  acknowledgeChange(generation: number, cursor: string): Promise<void>;
  subscribe(handler: (message: DesktopMessage) => void): Promise<UnlistenFn>;
  createWorkItem(input: CreateWorkItemInput): Promise<MutationReceipt>;
  completeWorkItem(input: ExistingWorkItemInput): Promise<MutationReceipt>;
  cancelWorkItem(input: ExistingWorkItemInput): Promise<MutationReceipt>;
  acknowledgeAttentionSignal(input: AcknowledgeSignalInput): Promise<MutationReceipt>;
  createReminder(input: CreateReminderInput): Promise<MutationReceipt>;
  acknowledgeReminderFire(input: AcknowledgeFireInput): Promise<MutationReceipt>;
  snoozeReminderFire(input: SnoozeFireInput): Promise<MutationReceipt>;
}

function command<T>(name: string, input: unknown): Promise<T> {
  return invoke<T>(name, { input }).catch((error: DesktopCommandError) => Promise.reject(error));
}

export const bridge: DesktopBridge = {
  state: () => invoke("desktop_state"),
  acknowledgeSnapshot: (generation, afterCursor) =>
    invoke("desktop_acknowledge_snapshot", { generation, afterCursor }),
  acknowledgeChange: (generation, cursor) =>
    invoke("desktop_acknowledge_change", { generation, cursor }),
  subscribe: async (handler) => {
    return listen<DesktopMessage>("attention://message", (event) => handler(event.payload));
  },
  createWorkItem: (input) => command("desktop_create_work_item", input),
  completeWorkItem: (input) => command("desktop_complete_work_item", input),
  cancelWorkItem: (input) => command("desktop_cancel_work_item", input),
  acknowledgeAttentionSignal: (input) => command("desktop_acknowledge_attention_signal", input),
  createReminder: (input) => command("desktop_create_reminder", input),
  acknowledgeReminderFire: (input) => command("desktop_acknowledge_reminder_fire", input),
  snoozeReminderFire: (input) => command("desktop_snooze_reminder_fire", input),
};
