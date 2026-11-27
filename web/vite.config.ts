import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const backendTarget = process.env.VITE_API_PROXY_TARGET ?? "http://127.0.0.1:1337";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    port: 5173,
    proxy: {
      "/api": backendTarget,
      "/admin": backendTarget,
      "/health": backendTarget,
      "/ready": backendTarget,
      "/docs": backendTarget,
      "/openapi.json": backendTarget,
      "/graphql": backendTarget,
    },
  },
});
