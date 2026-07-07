/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      // Live event stream served by `agentOS run` (SSE, AGENTOS_SSE_PORT).
      // Must be declared before the catch-all "/api" rule.
      "/api/events": {
        target: process.env.VITE_AGENTOS_SSE_TARGET || "http://127.0.0.1:8081",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api\/events/, "/events"),
      },
      // Health and inspection API served by `agentOS run` (AGENTOS_HTTP_PORT).
      "/api": {
        target: process.env.VITE_AGENTOS_API_TARGET || "http://127.0.0.1:8080",
        changeOrigin: true,
      },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
  },
});
