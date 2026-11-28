/// <reference types="node" />

/**
 * Playwright E2E configuration for RAPI Headless CMS.
 * Run with `npx playwright test` while `rapi serve` (port 1337) and `npm --prefix web run dev` (port 5173) are active.
 */
export default {
  testDir: "./e2e",
  testMatch: "**/*.e2e.ts",
  timeout: 45_000,
  retries: process.env.CI ? 1 : 0,
  use: {
    baseURL: process.env.RAPI_WEB_URL ?? "http://127.0.0.1:5173",
    trace: "on-first-retry",
    headless: true,
  },
};
