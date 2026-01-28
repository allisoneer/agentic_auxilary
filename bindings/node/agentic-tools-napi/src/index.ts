/* eslint-disable @typescript-eslint/no-explicit-any */
// Runtime implementation for agentic-tools-napi typed helpers.
// This file compiles to index.js at the package root and requires ./index.node.

const native = require('./index.node');

// Core functions (re-exported intact)
export const init = native.init;
export const listTools = native.listTools;
export const callTool = native.callTool;
export const setSchemaPatches = native.setSchemaPatches;
export const isInitialized = native.isInitialized;
export const toolCount = native.toolCount;
export const getToolNames = native.getToolNames;

// JSON-string per-tool wrappers (re-exported intact)
export const callLs = native.callLs;
export const callAskAgent = native.callAskAgent;
export const callGrep = native.callGrep;
export const callGlob = native.callGlob;
export const callJustSearch = native.callJustSearch;
export const callJustExecute = native.callJustExecute;
export const callReasoningRequest = native.callReasoningRequest;

// Internal helpers
function stringifyInput(input: unknown, label: string): string {
  try {
    return JSON.stringify(input ?? {});
  } catch (e: any) {
    const err = new Error(`${label}: failed to serialize input to JSON: ${e?.message ?? e}`);
    (err as any).cause = e;
    throw err;
  }
}

function parseResult<T = any>(raw: string, label: string): T {
  try {
    return JSON.parse(raw) as T;
  } catch (e: any) {
    const preview =
      typeof raw === 'string' && raw.length > 800
        ? `${raw.slice(0, 800)}... (${raw.length} chars)`
        : raw;
    const err = new Error(`${label}: failed to parse JSON result: ${e?.message ?? e}. Raw: ${preview}`);
    (err as any).cause = e;
    throw err;
  }
}

type ToolCallResult = { data: string; text: string };

async function callTyped<TOut = any>(
  fn: (json: string) => Promise<ToolCallResult>,
  input: unknown,
  label: string
): Promise<TOut> {
  const argsJson = stringifyInput(input, label);
  const result = await fn(argsJson);
  return parseResult<TOut>(result.data, label);
}

// Typed helper implementations (runtime-typed; definitions are in index.d.ts)
export async function callLsTyped(input: unknown): Promise<any> {
  return callTyped(native.callLs, input, 'callLsTyped');
}

export async function callAskAgentTyped(input: unknown): Promise<any> {
  return callTyped(native.callAskAgent, input, 'callAskAgentTyped');
}

export async function callGrepTyped(input: unknown): Promise<any> {
  return callTyped(native.callGrep, input, 'callGrepTyped');
}

export async function callGlobTyped(input: unknown): Promise<any> {
  return callTyped(native.callGlob, input, 'callGlobTyped');
}

export async function callJustSearchTyped(input: unknown): Promise<any> {
  return callTyped(native.callJustSearch, input, 'callJustSearchTyped');
}

export async function callJustExecuteTyped(input: unknown): Promise<any> {
  return callTyped(native.callJustExecute, input, 'callJustExecuteTyped');
}

export async function callReasoningRequestTyped(input: unknown): Promise<any> {
  return callTyped(native.callReasoningRequest, input, 'callReasoningRequestTyped');
}
