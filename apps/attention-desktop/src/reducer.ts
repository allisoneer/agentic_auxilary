import type {
  ChangeEvent,
  DesktopCommandError,
  DesktopMessage,
  DesktopState,
  InboxEntry,
  InboxKey,
  MutationKind,
  MutationReceipt,
  SnapshotState,
} from "./presentation";

export type MutationState =
  | { status: "submitting"; kind: MutationKind }
  | { status: "awaiting_sync"; kind: MutationKind; changeEventId: string; cursor: string }
  | {
      status: "conflict";
      kind: MutationKind;
      error: Extract<DesktopCommandError, { category: "expected_revision_conflict" }>;
    }
  | { status: "ambiguous" | "failed"; kind: MutationKind; message: string };

export interface AppState extends DesktopState {
  recentChangeEventIds: string[];
  mutations: Record<string, MutationState>;
  resetReason?: string;
}

export type AppAction =
  | { type: "desktop_message"; message: DesktopMessage }
  | { type: "mutation_started"; operationId: string; kind: MutationKind }
  | { type: "mutation_succeeded"; operationId: string; receipt: MutationReceipt }
  | { type: "mutation_failed"; operationId: string; error: DesktopCommandError }
  | { type: "dismiss_mutation"; operationId: string };

const RECENT_EVENT_LIMIT = 128;
const MUTATION_LIMIT = 64;

export const initialState: AppState = {
  sequence: 0,
  generation: 0,
  status: { kind: "connecting" },
  replay: [],
  recentChangeEventIds: [],
  mutations: {},
};

function key(entry: InboxEntry): string {
  switch (entry.kind) {
    case "work_item":
      return `work_item:${entry.workItem.id}`;
    case "attention_signal":
      return `attention_signal:${entry.attentionSignal.id}`;
    case "reminder_fire":
      return `reminder_fire:${entry.fire.id}`;
  }
}

function removalKey(entry: InboxKey): string {
  switch (entry.kind) {
    case "work_item":
      return `work_item:${entry.workItemId}`;
    case "attention_signal":
      return `attention_signal:${entry.attentionSignalId}`;
    case "reminder_fire":
      return `reminder_fire:${entry.reminderFireId}`;
  }
}

function upsert<T extends { id: string }>(items: T[], item: T): T[] {
  const index = items.findIndex(({ id }) => id === item.id);
  if (index < 0) return [...items, item];
  const next = [...items];
  next[index] = item;
  return next;
}

export function applyChange(snapshot: SnapshotState, event: ChangeEvent): SnapshotState {
  let next = { ...snapshot };
  for (const affected of event.affected) {
    if (affected.kind === "work_item")
      next = { ...next, workItems: upsert(next.workItems, affected.workItem) };
    if (affected.kind === "attention_signal")
      next = { ...next, attentionSignals: upsert(next.attentionSignals, affected.attentionSignal) };
    if (affected.kind === "reminder")
      next = { ...next, reminders: upsert(next.reminders, affected.reminder) };
  }
  const removed = new Set(event.inbox.removals.map(removalKey));
  const inbox = next.inbox.filter((entry) => !removed.has(key(entry)));
  for (const entry of event.inbox.upserts) {
    const index = inbox.findIndex((candidate) => key(candidate) === key(entry));
    if (index < 0) inbox.push(entry);
    else inbox[index] = entry;
  }
  return { ...next, inbox };
}

export function reconcileMutationsOnSnapshot(
  mutations: Record<string, MutationState>,
): Record<string, MutationState> {
  return Object.fromEntries(
    Object.entries(mutations).filter(([, mutation]) => mutation.status !== "awaiting_sync"),
  );
}

export function reduceMessage(state: AppState, message: DesktopMessage): AppState {
  if (message.sequence <= state.sequence || message.generation < state.generation) return state;
  if (message.type !== "reset" && message.generation > state.generation) return state;
  switch (message.type) {
    case "status":
      return { ...state, sequence: message.sequence, status: message.status };
    case "issue":
      return { ...state, sequence: message.sequence, issue: message.issue };
    case "reset":
      return {
        ...state,
        sequence: message.sequence,
        generation: message.generation,
        status: { kind: "gap" },
        snapshot: undefined,
        snapshotAfterCursor: undefined,
        issue: undefined,
        replay: [],
        recentChangeEventIds: [],
        resetReason: message.reason,
      };
    case "snapshot":
      return {
        ...state,
        sequence: message.sequence,
        generation: message.generation,
        snapshot: message.state,
        snapshotAfterCursor: message.afterCursor,
        issue: undefined,
        resetReason: undefined,
        mutations: reconcileMutationsOnSnapshot(state.mutations),
      };
    case "change": {
      if (!state.snapshot) return state;
      const mutations = { ...state.mutations };
      for (const [id, mutation] of Object.entries(mutations)) {
        if (mutation.status === "awaiting_sync" && mutation.changeEventId === message.event.id)
          delete mutations[id];
      }
      return {
        ...state,
        sequence: message.sequence,
        snapshot: applyChange(state.snapshot, message.event),
        recentChangeEventIds: [...state.recentChangeEventIds, message.event.id].slice(
          -RECENT_EVENT_LIMIT,
        ),
        mutations,
      };
    }
  }
}

function boundedMutations(mutations: Record<string, MutationState>): Record<string, MutationState> {
  const entries = Object.entries(mutations);
  return entries.length <= MUTATION_LIMIT
    ? mutations
    : Object.fromEntries(entries.slice(-MUTATION_LIMIT));
}

export function reducer(state: AppState, action: AppAction): AppState {
  if (action.type === "desktop_message") return reduceMessage(state, action.message);
  const mutations = { ...state.mutations };
  if (action.type === "dismiss_mutation") delete mutations[action.operationId];
  if (action.type === "mutation_started")
    mutations[action.operationId] = { status: "submitting", kind: action.kind };
  if (action.type === "mutation_succeeded") {
    const current = mutations[action.operationId];
    if (!current) return state;
    if (state.recentChangeEventIds.includes(action.receipt.changeEventId))
      delete mutations[action.operationId];
    else
      mutations[action.operationId] = {
        status: "awaiting_sync",
        kind: current.kind,
        changeEventId: action.receipt.changeEventId,
        cursor: action.receipt.cursor,
      };
  }
  if (action.type === "mutation_failed") {
    const current = mutations[action.operationId];
    if (!current) return state;
    if (action.error.category === "expected_revision_conflict")
      mutations[action.operationId] = {
        status: "conflict",
        kind: current.kind,
        error: action.error,
      };
    else
      mutations[action.operationId] = {
        status: action.error.category === "ambiguous_mutation" ? "ambiguous" : "failed",
        kind: current.kind,
        message: action.error.message,
      };
  }
  return { ...state, mutations: boundedMutations(mutations) };
}
