import type { Plugin } from "@opencode-ai/plugin"

const pause = 0.8
const clear = 0.75
const cmd = "/contextpause continue"

type State = {
  usable?: number
  pct: number
  paused: boolean
  override: boolean
}

const safeHook = <T extends (...args: any[]) => Promise<void>>(name: string, fn: T) =>
  (async (...args: Parameters<T>) => {
    try {
      await fn(...args)
    } catch (err) {
      console.error(`[context-pause] ${name} failed`, err)
    }
  }) as T

export const ContextPausePlugin: Plugin = async (_ctx) => {
  const states = new Map<string, State>()

  const get = (sessionID: string) => {
    const cur = states.get(sessionID)
    if (cur) return cur
    const next: State = { pct: 0, paused: false, override: false }
    states.set(sessionID, next)
    return next
  }

  const log = (msg: string) => console.log(`[context-pause] ${msg}`)

  return {
    "chat.params": safeHook("chat.params", async (input) => {
      const limit = input.model.limit
      const usable = limit.input || limit.context - limit.output
      const state = get(input.sessionID)
      state.usable = usable
    }),

    event: safeHook("event", async ({ event }) => {
      if (event.type !== "message.updated") return
      const msg: any = event.properties.info
      if (msg.role !== "assistant") return
      if (!msg.time?.completed) return

      const state = get(msg.sessionID)
      if (!state.usable) return

      const input = msg.tokens?.input ?? 0
      const read = msg.tokens?.cache?.read ?? 0
      const prompt = input + read
      const ratio = prompt / state.usable
      const pct = Math.round(ratio * 100)
      state.pct = pct

      if (ratio <= clear) {
        if (!state.paused && !state.override) return
        state.paused = false
        state.override = false
        log(`cleared (${pct}%)`)
        return
      }

      if (ratio < pause) return
      if (state.override) return
      if (state.paused) return

      state.paused = true
      log(`paused (${pct}%)`)
    }),

    "experimental.chat.messages.transform": safeHook(
      "experimental.chat.messages.transform",
      async (_input, output) => {
        const last = output.messages[output.messages.length - 1]
        if (!last) return

        const state = states.get(last.info.sessionID)
        if (!state) return
        if (!state.paused) return
        if (state.override) return

        const tpl = output.messages.slice().reverse().find((m) => m.info.role === "user")
        if (!tpl) return

        const id = crypto.randomUUID()
        const info: any = { ...tpl.info, id, time: { ...tpl.info.time, created: Date.now() } }

        const text =
          `Context usage is at ${state.pct}% (pause at 80%, clear at 75%). ` +
          `Please do a quick checkpoint: ` +
          `1) briefly summarize the current state/progress, ` +
          `2) update/verify the todo list, ` +
          `3) propose the next steps, then STOP and wait for the user. ` +
          `Do not call tools. ` +
          `Override command: "${cmd}".`

        const part: any = {
          id: crypto.randomUUID(),
          type: "text",
          text,
          sessionID: info.sessionID,
          messageID: id,
        }

        output.messages.push({ info, parts: [part] })
      },
    ),

    "chat.message": safeHook("chat.message", async (input, output) => {
      const part: any = output.parts.find((p: any) => p.type === "text")
      if (!part?.text) return

      const raw = part.text.trimStart()
      if (!raw.startsWith(cmd)) return

      const next = raw.slice(cmd.length).trimStart()
      const state = states.get(input.sessionID)
      if (!state?.paused) {
        part.text = next || "Continue."
        return
      }

      state.override = true
      log("override enabled")
      part.text = next || "Continue."
    }),
  }
}
