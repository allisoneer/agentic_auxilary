import { test, expect, beforeAll } from 'bun:test';
import { mkdtempSync, writeFileSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

// Native NAPI bindings. Requires the native binary to be built first (bun run build:debug).
const tools = require('..');

function makeFixture() {
  const dir = mkdtempSync(join(tmpdir(), 'agentic-tools-napi-'));
  writeFileSync(join(dir, 'a.txt'), 'hello world\n', 'utf8');
  mkdirSync(join(dir, 'subdir'));
  writeFileSync(join(dir, 'subdir', 'b.txt'), 'hello again\n', 'utf8');
  return dir;
}

function cleanupFixture(dir) {
  rmSync(dir, { recursive: true, force: true });
}

beforeAll(() => {
  if (!tools.isInitialized()) tools.init('{}');
});

test('native callLs returns ToolCallResult { data: string, text: string }', async () => {
  const dir = makeFixture();
  try {
    const result = await tools.callLs(JSON.stringify({ path: dir, depth: 1 }));
    expect(typeof result).toBe('object');
    expect(typeof result.data).toBe('string');
    expect(typeof result.text).toBe('string');

    const parsed = JSON.parse(result.data);
    expect(resolve(parsed.root)).toBe(resolve(dir));
    expect(Array.isArray(parsed.entries)).toBe(true);
    expect(typeof parsed.has_more).toBe('boolean');
    expect(Array.isArray(parsed.warnings)).toBe(true);
  } finally {
    cleanupFixture(dir);
  }
});

test('callLsTyped parses ToolCallResult.data and returns LsOutput object', async () => {
  const dir = makeFixture();
  try {
    const out = await tools.callLsTyped({ path: dir, depth: 1 });
    expect(resolve(out.root)).toBe(resolve(dir));
    expect(Array.isArray(out.entries)).toBe(true);
    expect(out.entries.some((e) => typeof e.path === 'string' && e.path.endsWith('a.txt'))).toBe(true);
  } finally {
    cleanupFixture(dir);
  }
});

// NOTE: callGrepTyped delegates to cli_grep which wraps ripgrep (rg).
// This test requires ripgrep to be installed on the host machine.
// If rg is not available, this test will fail for environmental reasons, not code reasons.
test('callGrepTyped parses ToolCallResult.data and returns GrepOutput object', async () => {
  const dir = makeFixture();
  try {
    const out = await tools.callGrepTyped({ pattern: 'hello', path: dir, mode: 'files' });
    expect(resolve(out.root)).toBe(resolve(dir));
    expect(out.mode).toBe('files');
    expect(Array.isArray(out.lines)).toBe(true);
    expect(out.lines.some((line) => typeof line === 'string' && line.endsWith('a.txt'))).toBe(true);
  } finally {
    cleanupFixture(dir);
  }
});
