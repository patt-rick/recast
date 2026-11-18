/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "chrome105",
    outDir: "dist",
    rollupOptions: {
      // Relative to the project root; no node imports (there is no @types/node).
      input: { main: "index.html", pad: "pad.html" },
    },
  },
  test: { environment: "jsdom", globals: true },
});
