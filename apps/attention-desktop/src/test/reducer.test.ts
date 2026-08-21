import { describe, expect, it } from "vitest";
import type { DesktopMessage, SnapshotState, WorkItem } from "../presentation";
import type { AppState } from "../reducer";
import { reduceMessage, reducer } from "../reducer";

const first: WorkItem = { id: "first", revision: "1", lifecycle: "open" };
const second: WorkItem = { id: "second", revision: "1", lifecycle: "open" };
const snapshot: SnapshotState = {
  workItems: [first, second],
  attentionSignals: [],
  reminders: [],
  inbox: [
    { kind: "work_item", workItem: first },
    { kind: "work_item", workItem: second },
  ],
};

const state: AppState = {
  sequence: 1,
  generation: 1,
  status: { kind: "connected" },
  snapshot,
  replay: [],
  recentChangeEventIds: [],
  mutations: {},
};

describe("authoritative desktop reducer", () => {
  it("records the cursor belonging to a replacement snapshot", () => {
    const message: DesktopMessage = {
      type: "snapshot",
      sequence: 2,
      generation: 1,
      state: snapshot,
      afterCursor: "cursor-2",
    };
    expect(reduceMessage(state, message).snapshotAfterCursor).toBe("cursor-2");
  });

  it("replaces affected roots and inbox entries in place and obeys exact effects", () => {
    const updated = { ...first, revision: "2" };
    const message: DesktopMessage = {
      type: "change",
      sequence: 2,
      generation: 1,
      event: {
        id: "event-2",
        cursor: "cursor-2",
        occurredAt: "2026-01-01T00:00:00Z",
        kind: "work_item_changed",
        affected: [{ kind: "work_item", workItem: updated }],
        inbox: {
          upserts: [{ kind: "work_item", workItem: updated }],
          removals: [],
        },
      },
    };
    const next = reduceMessage(state, message);
    expect(next.snapshot?.workItems).toEqual([updated, second]);
    expect(next.snapshot?.inbox).toEqual([
      { kind: "work_item", workItem: updated },
      { kind: "work_item", workItem: second },
    ]);
  });

  it("does not infer inbox removal from lifecycle", () => {
    const completed = { ...first, revision: "2", lifecycle: "completed" as const };
    const next = reduceMessage(state, {
      type: "change",
      sequence: 2,
      generation: 1,
      event: {
        id: "event-2",
        cursor: "cursor-2",
        occurredAt: "2026-01-01T00:00:00Z",
        kind: "work_item_completed",
        affected: [{ kind: "work_item", workItem: completed }],
        inbox: { upserts: [], removals: [] },
      },
    });
    expect(next.snapshot?.inbox).toEqual(snapshot.inbox);
  });

  it("correlates mutation results whether the event or response arrives first", () => {
    const started = reducer(state, {
      type: "mutation_started",
      operationId: "complete:first",
      kind: "complete_work_item",
    });
    const receipt = {
      disposition: "applied" as const,
      cursor: "cursor-2",
      changeEventId: "event-2",
      resource: { kind: "work_item" as const, id: "first" },
    };
    const awaiting = reducer(started, {
      type: "mutation_succeeded",
      operationId: "complete:first",
      receipt,
    });
    expect(awaiting.mutations["complete:first"]?.status).toBe("awaiting_sync");
    const event: DesktopMessage = {
      type: "change",
      sequence: 2,
      generation: 1,
      event: {
        id: "event-2",
        cursor: "cursor-2",
        occurredAt: "now",
        kind: "changed",
        affected: [],
        inbox: { upserts: [], removals: [] },
      },
    };
    expect(reduceMessage(awaiting, event).mutations).toEqual({});

    const eventFirst = reduceMessage(started, event);
    expect(
      reducer(eventFirst, { type: "mutation_succeeded", operationId: "complete:first", receipt })
        .mutations,
    ).toEqual({});
  });

  it("keeps in-flight work across reset but reconciles successful work on replacement snapshot", () => {
    const submitting = reducer(state, {
      type: "mutation_started",
      operationId: "create",
      kind: "create_work_item",
    });
    const awaiting = reducer(submitting, {
      type: "mutation_succeeded",
      operationId: "create",
      receipt: {
        disposition: "applied",
        cursor: "cursor-2",
        changeEventId: "event-2",
        resource: { kind: "work_item", id: "created" },
      },
    });
    const reset = reduceMessage(awaiting, {
      type: "reset",
      sequence: 2,
      generation: 2,
      reason: "gap",
    });
    expect(reset.snapshot).toBeUndefined();
    expect(reset.snapshotAfterCursor).toBeUndefined();
    expect(reset.mutations.create?.status).toBe("awaiting_sync");
    expect(reset.status.kind).toBe("gap");
    const refreshed = reduceMessage(reset, {
      type: "snapshot",
      sequence: 3,
      generation: 2,
      state: snapshot,
      afterCursor: "fresh",
    });
    expect(refreshed.mutations).toEqual({});
  });

  it("bounds event correlation and ignores duplicate sequence effects", () => {
    let next = state;
    for (let index = 0; index < 140; index += 1) {
      next = reduceMessage(next, {
        type: "change",
        sequence: index + 2,
        generation: 1,
        event: {
          id: `event-${index}`,
          cursor: `${index}`,
          occurredAt: "now",
          kind: "changed",
          affected: [],
          inbox: { upserts: [], removals: [] },
        },
      });
    }
    expect(next.recentChangeEventIds).toHaveLength(128);
    const duplicate = reduceMessage(next, {
      type: "change",
      sequence: 141,
      generation: 1,
      event: {
        id: "duplicate",
        cursor: "x",
        occurredAt: "now",
        kind: "changed",
        affected: [],
        inbox: { upserts: [{ kind: "work_item", workItem: second }], removals: [] },
      },
    });
    expect(duplicate).toBe(next);
  });
});
