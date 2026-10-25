/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync } from "node:fs";
import { phoneLandscapeCss } from "./src/lib/phoneLandscapeCss";

const pkg = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8")) as { version: string };

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  define: { __APP_VERSION__: JSON.stringify(pkg.version) },
  // U9b: phones keep the narrow layout in a landscape wider than 899 px (src/lib/phoneLandscapeCss.ts).
  css: { postcss: { plugins: [phoneLandscapeCss()] } },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
  },
  test: { environment: "jsdom", globals: true },
});
