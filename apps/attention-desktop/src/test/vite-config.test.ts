// @vitest-environment node

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

describe("desktop Vite configuration", () => {
  it("enables debug output only for the explicit true value", () => {
    const testDirectory = dirname(fileURLToPath(import.meta.url));
    const configSource = readFileSync(resolve(testDirectory, "../../vite.config.ts"), "utf8");

    expect(configSource).toContain('const debug = debugValue === "true";');
    expect(configSource).toContain('minify: debug ? false : "esbuild",');
    expect(configSource).toContain("sourcemap: debug,");
  });
});
