import { type FormEvent, useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { bridge, type DesktopBridge } from "./bridge";
import type {
  DesktopCommandError,
  DesktopMessage,
  DesktopState,
  MutationKind,
  MutationReceipt,
  ReminderTarget,
} from "./presentation";
import {
  type AppAction,
  type AppState,
  initialState,
  reconcileMutationsOnSnapshot,
  reduceMessage,
  reducer,
} from "./reducer";

const recoveryIssue = {
  category: "bridge",
  message: "Desktop synchronization failed; recovering from a fresh state.",
};

function operationStatus(state: AppState, id: string) {
  return state.mutations[id];
}

export function canonicalizeLocalDateTime(value: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,3}))?)?$/.exec(
    value,
  );
  if (!match) throw new Error("Enter a valid local date and time.");
  const [, year, month, day, hour, minute, second = "0", fraction = "0"] = match;
  const date = new Date(
    Number(year),
    Number(month) - 1,
    Number(day),
    Number(hour),
    Number(minute),
    Number(second),
    Number(fraction.padEnd(3, "0")),
  );
  if (
    Number.isNaN(date.valueOf()) ||
    date.getFullYear() !== Number(year) ||
    date.getMonth() !== Number(month) - 1 ||
    date.getDate() !== Number(day) ||
    date.getHours() !== Number(hour) ||
    date.getMinutes() !== Number(minute) ||
    date.getSeconds() !== Number(second)
  )
    throw new Error("Enter a valid local date and time.");
  return date.toISOString().replace(/\.(\d{3})Z$/, ".$1000Z");
}

export function App({ api = bridge }: { api?: DesktopBridge }) {
  const [state, setState] = useState(initialState);
  const stateRef = useRef(state);
  stateRef.current = state;

  const commit = useCallback((next: AppState) => {
    stateRef.current = next;
    flushSync(() => setState(next));
  }, []);
  const dispatch = (action: AppAction) => commit(reducer(stateRef.current, action));

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    let ready = false;
    let buffered: DesktopMessage[] = [];
    let serial = Promise.resolve();

    const acknowledge = async (message: DesktopMessage) => {
      if (message.type === "snapshot")
        await api.acknowledgeSnapshot(message.generation, message.afterCursor);
      if (message.type === "change")
        await api.acknowledgeChange(message.generation, message.event.cursor);
    };
    const apply = async (message: DesktopMessage) => {
      if (!active) return;
      const before = stateRef.current;
      const after = reduceMessage(before, message);
      if (after === before) return;
      commit(after);
      await acknowledge(message);
    };
    const commitBootstrap = async (current: DesktopState) => {
      const replay = [...current.replay].sort((a, b) => a.sequence - b.sequence);
      let next: AppState = {
        ...stateRef.current,
        ...current,
        sequence: replay[0] ? replay[0].sequence - 1 : current.sequence,
        generation: replay[0]?.generation ?? current.generation,
        replay: [],
        mutations: current.snapshot
          ? reconcileMutationsOnSnapshot(stateRef.current.mutations)
          : stateRef.current.mutations,
      };
      const applied: DesktopMessage[] = [];
      for (const message of replay) {
        const reduced = reduceMessage(next, message);
        if (reduced !== next) {
          next = reduced;
          applied.push(message);
        }
      }
      commit(next);
      for (const message of applied) await acknowledge(message);
    };
    const recover = async () => {
      if (!active) return;
      commit({ ...stateRef.current, issue: recoveryIssue });
      const current = await api.state();
      if (active && current.sequence >= stateRef.current.sequence) await commitBootstrap(current);
    };
    const enqueue = (message: DesktopMessage) => {
      serial = serial.then(() => apply(message)).catch(() => recover());
    };
    void (async () => {
      unlisten = await api.subscribe((message) =>
        ready ? enqueue(message) : buffered.push(message),
      );
      const current = await api.state();
      if (!active) return;
      await commitBootstrap(current);
      ready = true;
      for (const message of buffered.sort((a, b) => a.sequence - b.sequence)) enqueue(message);
      buffered = [];
    })().catch(() => {
      ready = true;
      void recover();
    });
    return () => {
      active = false;
      buffered = [];
      unlisten?.();
    };
  }, [api, commit]);

  const run = async (
    operationId: string,
    kind: MutationKind,
    command: () => Promise<MutationReceipt>,
  ) => {
    dispatch({ type: "mutation_started", operationId, kind });
    try {
      dispatch({ type: "mutation_succeeded", operationId, receipt: await command() });
    } catch (error) {
      dispatch({ type: "mutation_failed", operationId, error: error as DesktopCommandError });
    }
  };
  const authoritative = Boolean(state.snapshot) && state.status.kind === "connected";

  return (
    <main>
      <header>
        <h1>Attention</h1>
        <span className="status">{state.status.kind}</span>
      </header>
      {!authoritative && (
        <p role="status">Waiting for a fresh snapshot; authoritative state is unavailable…</p>
      )}
      {state.issue && (
        <p role="alert" className="issue">
          {state.issue.message}
        </p>
      )}
      <MutationNotices
        state={state}
        dismiss={(id) => dispatch({ type: "dismiss_mutation", operationId: id })}
      />
      <CreateWorkItem api={api} disabled={!authoritative} run={run} state={state} />
      <CreateReminder api={api} disabled={!authoritative} run={run} state={state} />
      <section aria-labelledby="inbox-heading">
        <h2 id="inbox-heading">Inbox</h2>
        {!state.snapshot ? null : state.snapshot.inbox.length === 0 ? (
          <p>Your inbox is clear.</p>
        ) : (
          <ul className="inbox">
            {state.snapshot.inbox.map((entry) => {
              if (entry.kind === "work_item") {
                const { id, revision, lifecycle } = entry.workItem;
                return (
                  <li key={`work:${id}`}>
                    <article>
                      <h3>Work item</h3>
                      <p>{id}</p>
                      <p>
                        Revision {revision}; {lifecycle}
                      </p>
                      <button
                        type="button"
                        disabled={
                          !authoritative || Boolean(operationStatus(state, `complete:${id}`))
                        }
                        onClick={() =>
                          void run(`complete:${id}`, "complete_work_item", () =>
                            api.completeWorkItem({ id, expectedRevision: revision }),
                          )
                        }
                      >
                        Complete
                      </button>{" "}
                      <button
                        type="button"
                        disabled={!authoritative || Boolean(operationStatus(state, `cancel:${id}`))}
                        onClick={() =>
                          void run(`cancel:${id}`, "cancel_work_item", () =>
                            api.cancelWorkItem({ id, expectedRevision: revision }),
                          )
                        }
                      >
                        Cancel
                      </button>
                    </article>
                  </li>
                );
              }
              if (entry.kind === "attention_signal") {
                const { id, revision, sourceLifecycle, attentionState } = entry.attentionSignal;
                return (
                  <li key={`signal:${id}`}>
                    <article>
                      <h3>Attention signal</h3>
                      <p>{id}</p>
                      <p>
                        {sourceLifecycle}; {attentionState}; revision {revision}
                      </p>
                      <button
                        type="button"
                        disabled={!authoritative || Boolean(operationStatus(state, `signal:${id}`))}
                        onClick={() =>
                          void run(`signal:${id}`, "acknowledge_attention_signal", () =>
                            api.acknowledgeAttentionSignal({ id, expectedRevision: revision }),
                          )
                        }
                      >
                        Acknowledge
                      </button>
                    </article>
                  </li>
                );
              }
              const operation = `fire:${entry.fire.id}`;
              return (
                <li key={operation}>
                  <article>
                    <h3>Reminder fire</h3>
                    <p>{entry.fire.id}</p>
                    <p>
                      Triggered {entry.fire.triggerAt}; revision {entry.reminderRevision}
                    </p>
                    <button
                      type="button"
                      disabled={!authoritative || Boolean(operationStatus(state, operation))}
                      onClick={() =>
                        void run(operation, "acknowledge_reminder_fire", () =>
                          api.acknowledgeReminderFire({
                            reminderId: entry.reminderId,
                            fireId: entry.fire.id,
                            expectedRevision: entry.reminderRevision,
                          }),
                        )
                      }
                    >
                      Acknowledge
                    </button>
                    <Snooze
                      api={api}
                      entry={entry}
                      disabled={!authoritative}
                      run={run}
                      state={state}
                    />
                  </article>
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </main>
  );
}

type Runner = (
  id: string,
  kind: MutationKind,
  command: () => Promise<MutationReceipt>,
) => Promise<void>;

function CreateWorkItem({
  api,
  disabled,
  run,
  state,
}: {
  api: DesktopBridge;
  disabled: boolean;
  run: Runner;
  state: AppState;
}) {
  const [dueAt, setDueAt] = useState("");
  const [scheduledAt, setScheduledAt] = useState("");
  const [deferUntil, setDeferUntil] = useState("");
  const [validationError, setValidationError] = useState<string>();
  const id = "create-work-item";
  const submit = (event: FormEvent) => {
    event.preventDefault();
    try {
      const input = {
        dueAt: dueAt ? canonicalizeLocalDateTime(dueAt) : undefined,
        scheduledAt: scheduledAt ? canonicalizeLocalDateTime(scheduledAt) : undefined,
        deferUntil: deferUntil ? canonicalizeLocalDateTime(deferUntil) : undefined,
      };
      setValidationError(undefined);
      void run(id, "create_work_item", () => api.createWorkItem(input));
    } catch (error) {
      setValidationError((error as Error).message);
    }
  };
  return (
    <section>
      <h2>Create work item</h2>
      <form onSubmit={submit}>
        {validationError && <p role="alert">{validationError}</p>}
        <label>
          Due{" "}
          <input type="datetime-local" value={dueAt} onChange={(e) => setDueAt(e.target.value)} />
        </label>
        <label>
          Scheduled{" "}
          <input
            type="datetime-local"
            value={scheduledAt}
            onChange={(e) => setScheduledAt(e.target.value)}
          />
        </label>
        <label>
          Defer until{" "}
          <input
            type="datetime-local"
            value={deferUntil}
            onChange={(e) => setDeferUntil(e.target.value)}
          />
        </label>
        <button type="submit" disabled={disabled || Boolean(operationStatus(state, id))}>
          Create work item
        </button>
      </form>
    </section>
  );
}

function CreateReminder({
  api,
  disabled,
  run,
  state,
}: {
  api: DesktopBridge;
  disabled: boolean;
  run: Runner;
  state: AppState;
}) {
  const [target, setTarget] = useState("");
  const [triggerAt, setTriggerAt] = useState("");
  const [validationError, setValidationError] = useState<string>();
  const id = "create-reminder";
  const options: { value: string; label: string; target: ReminderTarget }[] = [
    ...(state.snapshot?.workItems.map((item) => ({
      value: `work:${item.id}`,
      label: `Work item ${item.id}`,
      target: { kind: "work_item" as const, workItemId: item.id },
    })) ?? []),
    ...(state.snapshot?.attentionSignals.map((signal) => ({
      value: `signal:${signal.id}`,
      label: `Signal ${signal.id}`,
      target: { kind: "attention_signal" as const, attentionSignalId: signal.id },
    })) ?? []),
  ];
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const selected = options.find(({ value }) => value === target);
    if (!selected) return;
    try {
      const canonicalTriggerAt = canonicalizeLocalDateTime(triggerAt);
      setValidationError(undefined);
      void run(id, "create_reminder", () =>
        api.createReminder({ target: selected.target, triggerAt: canonicalTriggerAt }),
      );
    } catch (error) {
      setValidationError((error as Error).message);
    }
  };
  return (
    <section>
      <h2>Create reminder</h2>
      <form onSubmit={submit}>
        {validationError && <p role="alert">{validationError}</p>}
        <label>
          Target{" "}
          <select required value={target} onChange={(e) => setTarget(e.target.value)}>
            <option value="">Select target</option>
            {options.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Trigger at{" "}
          <input
            required
            type="datetime-local"
            value={triggerAt}
            onChange={(e) => setTriggerAt(e.target.value)}
          />
        </label>
        <button type="submit" disabled={disabled || !target || Boolean(operationStatus(state, id))}>
          Create reminder
        </button>
      </form>
    </section>
  );
}

function Snooze({
  api,
  entry,
  disabled,
  run,
  state,
}: {
  api: DesktopBridge;
  entry: Extract<NonNullable<AppState["snapshot"]>["inbox"][number], { kind: "reminder_fire" }>;
  disabled: boolean;
  run: Runner;
  state: AppState;
}) {
  const [replacementTriggerAt, setReplacement] = useState("");
  const [validationError, setValidationError] = useState<string>();
  const id = `snooze:${entry.fire.id}`;
  return (
    <form
      className="inline"
      onSubmit={(event) => {
        event.preventDefault();
        try {
          const canonicalReplacement = canonicalizeLocalDateTime(replacementTriggerAt);
          setValidationError(undefined);
          void run(id, "snooze_reminder_fire", () =>
            api.snoozeReminderFire({
              reminderId: entry.reminderId,
              fireId: entry.fire.id,
              expectedRevision: entry.reminderRevision,
              replacementTriggerAt: canonicalReplacement,
            }),
          );
        } catch (error) {
          setValidationError((error as Error).message);
        }
      }}
    >
      {validationError && <p role="alert">{validationError}</p>}
      <label>
        Snooze until{" "}
        <input
          required
          type="datetime-local"
          value={replacementTriggerAt}
          onChange={(e) => setReplacement(e.target.value)}
        />
      </label>
      <button type="submit" disabled={disabled || Boolean(operationStatus(state, id))}>
        Snooze
      </button>
    </form>
  );
}

function MutationNotices({ state, dismiss }: { state: AppState; dismiss: (id: string) => void }) {
  return (
    <div aria-live="polite">
      {Object.entries(state.mutations).map(([id, mutation]) => (
        <p
          className={
            mutation.status === "conflict" ||
            mutation.status === "ambiguous" ||
            mutation.status === "failed"
              ? "issue"
              : ""
          }
          key={id}
        >
          {mutation.status === "submitting" && "Submitting…"}
          {mutation.status === "awaiting_sync" && "Waiting for synchronized update…"}
          {mutation.status === "conflict" &&
            `This item changed from revision ${mutation.error.expectedRevision} to ${mutation.error.actualRevision}. Review the refreshed item before trying again.`}
          {mutation.status === "ambiguous" &&
            `Outcome unknown: ${mutation.message}. Wait for synchronization before trying again.`}
          {mutation.status === "failed" && mutation.message}
          {(mutation.status === "conflict" ||
            mutation.status === "ambiguous" ||
            mutation.status === "failed") && (
            <button type="button" onClick={() => dismiss(id)}>
              Dismiss
            </button>
          )}
        </p>
      ))}
    </div>
  );
}
