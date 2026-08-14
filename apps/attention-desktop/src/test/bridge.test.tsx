import { act, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { App } from "../App";
import type { DesktopBridge } from "../bridge";
import type { DesktopMessage, DesktopState, SnapshotState } from "../presentation";

const empty: SnapshotState = { workItems: [], attentionSignals: [], reminders: [], inbox: [] };

const workChange: DesktopMessage = {
  type: "change",
  sequence: 2,
  generation: 1,
  event: {
    id: "e",
    cursor: "11",
    occurredAt: "2026-01-01T00:00:00.000000Z",
    kind: "work_item_created",
    affected: [],
    inbox: {
      upserts: [
        { kind: "work_item", workItem: { id: "work-1", revision: "1", lifecycle: "open" } },
      ],
      removals: [],
    },
  },
};

function mockBridge() {
  let handler: ((message: DesktopMessage) => void) | undefined;
  const calls: string[] = [];
  const unlisten = vi.fn();
  const state: DesktopState = {
    sequence: 0,
    generation: 1,
    status: { kind: "connected" },
    replay: [],
  };
  const api: DesktopBridge = {
    state: vi.fn(async () => {
      calls.push("state");
      return state;
    }),
    acknowledgeSnapshot: vi.fn(async () => {
      calls.push("snapshot-ack");
    }),
    acknowledgeChange: vi.fn(async () => {
      calls.push("change-ack");
    }),
    subscribe: vi.fn(async (next) => {
      handler = next;
      return unlisten;
    }),
    createWorkItem: vi.fn(),
    completeWorkItem: vi.fn(),
    cancelWorkItem: vi.fn(),
    acknowledgeAttentionSignal: vi.fn(),
    createReminder: vi.fn(),
    acknowledgeReminderFire: vi.fn(),
    snoozeReminderFire: vi.fn(),
  };
  return {
    api,
    calls,
    unlisten,
    emit: (message: DesktopMessage) => {
      if (!handler) throw new Error("not subscribed");
      handler(message);
    },
  };
}

describe("desktop shell", () => {
  it("requests initial state and applies snapshot before acknowledgement", async () => {
    const mock = mockBridge();
    render(<App api={mock.api} />);
    await waitFor(() => expect(mock.api.state).toHaveBeenCalledOnce());
    await act(async () =>
      mock.emit({ type: "snapshot", sequence: 1, generation: 1, state: empty, afterCursor: "10" }),
    );
    expect(screen.getByText("Your inbox is clear.")).toBeVisible();
    expect(mock.api.acknowledgeSnapshot).toHaveBeenCalledWith(1, "10");
    expect(mock.calls.at(-1)).toBe("snapshot-ack");
  });

  it("applies changes before acknowledging their cursor", async () => {
    const mock = mockBridge();
    render(<App api={mock.api} />);
    await waitFor(() => expect(mock.api.state).toHaveBeenCalled());
    await act(async () =>
      mock.emit({ type: "snapshot", sequence: 1, generation: 1, state: empty, afterCursor: "10" }),
    );
    await act(async () => mock.emit(workChange));
    expect(screen.getByText(/work-1/)).toBeVisible();
    expect(mock.api.acknowledgeChange).toHaveBeenCalledWith(1, "11");
  });

  it("resets on gap and ignores stale sequence and generation", async () => {
    const mock = mockBridge();
    render(<App api={mock.api} />);
    await waitFor(() => expect(mock.api.state).toHaveBeenCalled());
    await act(async () =>
      mock.emit({ type: "snapshot", sequence: 2, generation: 1, state: empty, afterCursor: "10" }),
    );
    await act(async () => mock.emit({ type: "reset", sequence: 3, generation: 2, reason: "gap" }));
    expect(screen.getByText(/Waiting for a fresh snapshot/)).toBeVisible();
    await act(async () =>
      mock.emit({ type: "snapshot", sequence: 4, generation: 1, state: empty, afterCursor: "old" }),
    );
    await act(async () =>
      // Sequence 3 deliberately duplicates the reset sequence to verify stale suppression.
      mock.emit({ type: "status", sequence: 3, generation: 2, status: { kind: "closed" } }),
    );
    expect(mock.api.acknowledgeSnapshot).not.toHaveBeenCalledWith(1, "old");
    expect(screen.getByText("gap")).toBeVisible();
  });

  it("atomically folds replay while duplicate subscribed events are buffered", async () => {
    const mock = mockBridge();
    const snapshot: DesktopMessage = {
      type: "snapshot",
      sequence: 1,
      generation: 1,
      state: empty,
      afterCursor: "10",
    };
    let resolveState: ((state: DesktopState) => void) | undefined;
    vi.mocked(mock.api.state).mockImplementation(
      () => new Promise((resolve) => (resolveState = resolve)),
    );
    const view = render(<App api={mock.api} />);
    await waitFor(() => expect(mock.api.state).toHaveBeenCalledOnce());
    act(() => mock.emit(workChange));
    await act(async () =>
      resolveState?.({
        sequence: 2,
        generation: 1,
        status: { kind: "connected" },
        snapshot: empty,
        snapshotAfterCursor: "10",
        replay: [snapshot, workChange],
      }),
    );
    expect(view.container).toHaveTextContent("work-1");
    expect(mock.api.acknowledgeSnapshot).toHaveBeenCalledWith(1, "10");
    expect(mock.api.acknowledgeChange).toHaveBeenCalledTimes(1);
    expect(mock.calls.slice(-2)).toEqual(["snapshot-ack", "change-ack"]);
  });

  it("handles rejected state recovery during startup", async () => {
    const mock = mockBridge();
    vi.mocked(mock.api.state).mockRejectedValue(new Error("state unavailable"));
    render(<App api={mock.api} />);
    expect(await screen.findByRole("alert")).toHaveTextContent(/recovering from a fresh state/);
    await waitFor(() => expect(mock.api.state).toHaveBeenCalledTimes(2));
  });

  it("handles rejected state recovery after a serial acknowledgement failure", async () => {
    const mock = mockBridge();
    vi.mocked(mock.api.acknowledgeChange).mockRejectedValue(new Error("ack failed"));
    vi.mocked(mock.api.state)
      .mockResolvedValueOnce({
        sequence: 1,
        generation: 1,
        status: { kind: "connected" },
        snapshot: empty,
        replay: [],
      })
      .mockRejectedValueOnce(new Error("state unavailable"));
    render(<App api={mock.api} />);
    await waitFor(() => expect(mock.api.state).toHaveBeenCalledOnce());
    act(() => mock.emit(workChange));
    expect(await screen.findByRole("alert")).toHaveTextContent(/recovering from a fresh state/);
    await waitFor(() => expect(mock.api.state).toHaveBeenCalledTimes(2));
  });

  it("unlistens on unmount", async () => {
    const mock = mockBridge();
    const view = render(<App api={mock.api} />);
    await waitFor(() => expect(mock.api.subscribe).toHaveBeenCalled());
    view.unmount();
    expect(mock.unlisten).toHaveBeenCalledOnce();
  });

  it("unlistens exactly once when subscribe resolves after unmount", async () => {
    const mock = mockBridge();
    let resolveSubscribe: ((stop: () => void) => void) | undefined;
    vi.mocked(mock.api.subscribe).mockImplementation(
      () => new Promise((resolve) => (resolveSubscribe = resolve)),
    );
    const view = render(<App api={mock.api} />);
    await waitFor(() => expect(mock.api.subscribe).toHaveBeenCalledOnce());

    view.unmount();
    expect(mock.unlisten).not.toHaveBeenCalled();
    await act(async () => resolveSubscribe?.(mock.unlisten));

    expect(mock.unlisten).toHaveBeenCalledOnce();
  });
});
