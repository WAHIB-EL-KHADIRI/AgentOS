/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// When the runtime requires auth (AGENTOS_API_TOKEN), the dev proxy injects
// the Bearer header server-side so the token never reaches browser code.
const apiToken = process.env.AGENTOS_API_TOKEN;
const authHeaders = apiToken ? { Authorization: `Bearer ${apiToken}` } : undefined;

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
        headers: authHeaders,
      },
      // Health and inspection API served by `agentOS run` (AGENTOS_HTTP_PORT).
      "/api": {
        target: process.env.VITE_AGENTOS_API_TARGET || "http://127.0.0.1:8080",
        changeOrigin: true,
        headers: authHeaders,
      },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
  },
});
