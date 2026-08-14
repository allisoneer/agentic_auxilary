import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

export function desktopBuildOptions(debugValue: string | undefined): {
  minify: false | "esbuild";
  sourcemap: boolean;
} {
  const debug = debugValue === "true";
  return {
    minify: debug ? false : "esbuild",
    sourcemap: debug,
  };
}

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 5174 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    ...desktopBuildOptions(process.env.TAURI_ENV_DEBUG),
  },
});
