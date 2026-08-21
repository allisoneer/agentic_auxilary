import "@testing-library/jest-dom/vitest";
import { webcrypto } from "node:crypto";

if (!globalThis.crypto) Object.defineProperty(globalThis, "crypto", { value: webcrypto });
