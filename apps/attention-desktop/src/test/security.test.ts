import { readdirSync, readFileSync } from "node:fs";
import { basename, extname, join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = resolve(import.meta.dirname, "../..");
const tauriRoot = join(root, "src-tauri");

function filesBelow(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? filesBelow(path) : [path];
  });
}

function json(path: string): unknown {
  return JSON.parse(readFileSync(path, "utf8"));
}

describe("desktop static security boundary", () => {
  it("uses a restrictive local-only CSP and one main-window capability", () => {
    const config = json(join(tauriRoot, "tauri.conf.json")) as {
      app: {
        security: { capabilities: string[]; csp: Record<string, string> };
        windows: { label: string }[];
      };
      build: { devUrl: string };
    };
    const capability = json(join(tauriRoot, "capabilities/default.json")) as {
      windows: string[];
      permissions: string[];
    };

    expect(config.app.windows.map(({ label }) => label)).toEqual(["main"]);
    expect(config.build.devUrl).toMatch(/^http:\/\/(?:localhost|127\.0\.0\.1)(?::\d+)?\/?$/);
    expect(config.app.security.capabilities).toEqual(["default"]);
    expect(config.app.security.csp).toEqual({
      "default-src": "'self'",
      "script-src": "'self'",
      "style-src": "'self'",
      "img-src": "'self' data:",
      "font-src": "'self'",
      "connect-src": "ipc: http://ipc.localhost",
      "object-src": "'none'",
      "frame-src": "'none'",
      "base-uri": "'none'",
      "form-action": "'none'",
    });
    expect(capability.windows).toEqual(["main"]);
    expect(capability.permissions).toEqual([
      "core:event:allow-listen",
      "core:event:allow-unlisten",
      "allow-desktop-state",
      "allow-desktop-acknowledge-snapshot",
      "allow-desktop-acknowledge-change",
      "allow-desktop-create-work-item",
      "allow-desktop-complete-work-item",
      "allow-desktop-cancel-work-item",
      "allow-desktop-acknowledge-attention-signal",
      "allow-desktop-create-reminder",
      "allow-desktop-acknowledge-reminder-fire",
      "allow-desktop-snooze-reminder-fire",
    ]);
    expect(capability.permissions.join(" ")).not.toMatch(
      /shell|filesystem|(?:^|[^a-z])fs:|http:allow|webview:allow|window:allow-create/i,
    );
  });

  it("keeps Tauri and browser authority behind bridge.ts", () => {
    const sources = filesBelow(join(root, "src")).filter(
      (path) =>
        [".ts", ".tsx"].includes(extname(path)) && !path.includes(`${join("src", "test")}/`),
    );
    for (const path of sources) {
      const source = readFileSync(path, "utf8");
      if (basename(path) !== "bridge.ts") {
        expect(source, path).not.toContain("@tauri-apps/api");
      }
      expect(source, path).not.toMatch(
        /\bfetch\s*\(|\bWebSocket\b|\b(?:localStorage|sessionStorage|indexedDB)\b|dangerouslySetInnerHTML|\binnerHTML\s*=/,
      );
    }
  });

  it("has Bun as the only JavaScript package-manager lockfile", () => {
    expect(readFileSync(join(root, "package.json"), "utf8")).toContain('"bun@1.3.14"');
    const lockfiles = readdirSync(root).filter((name) =>
      ["bun.lock", "bun.lockb", "package-lock.json", "pnpm-lock.yaml", "yarn.lock"].includes(name),
    );
    expect(lockfiles).toEqual(["bun.lock"]);
  });
});
