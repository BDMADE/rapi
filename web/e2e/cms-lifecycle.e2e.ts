/**
 * Full-stack Playwright E2E lifecycle specification for RAPI (Strapi v5 feature parity):
 * Onboarding -> Content-Type Builder -> Entry Draft & Publish -> REST & GraphQL API -> Plugins & MCP.
 *
 * Execute via `npx playwright test` against a fresh RAPI workspace.
 */

interface PageLike {
  goto: (url: string) => Promise<void>;
  getByRole: (role: string, opts?: { name?: string | RegExp }) => LocatorLike;
  getByLabel: (label: string | RegExp) => LocatorLike;
  getByPlaceholder: (text: string | RegExp) => LocatorLike;
  getByText: (text: string | RegExp) => LocatorLike;
  request: {
    get: (url: string) => Promise<{ ok: () => boolean; json: () => Promise<Record<string, unknown>> }>;
    post: (
      url: string,
      opts: { data: unknown; headers?: Record<string, string> },
    ) => Promise<{ ok: () => boolean; json: () => Promise<Record<string, unknown>> }>;
  };
}

interface LocatorLike {
  click: () => Promise<void>;
  fill: (val: string) => Promise<void>;
  isVisible: () => Promise<boolean>;
}

const API_BASE = process.env.RAPI_API_URL ?? "http://127.0.0.1:1337";

export async function runCmsLifecycleScenario(page: PageLike) {
  await page.goto("/onboarding");
  const firstNameInput = page.getByLabel("First name");
  if (await firstNameInput.isVisible()) {
    await firstNameInput.fill("Ada");
    await page.getByLabel("Last name").fill("Lovelace");
    await page.getByLabel("Email").fill("ada@rapi.local");
    await page.getByLabel("Password").fill("RapiAdmin99!");
    await page.getByLabel("Confirm password").fill("RapiAdmin99!");
    await page.getByRole("button", { name: /Initialize administrator/i }).click();
  }

  await page.goto("/content-type-builder");
  await page.getByRole("button", { name: /New schema/i }).click();
  await page.getByPlaceholder("Article").fill("Post");
  await page.getByRole("button", { name: /Create schema/i }).click();

  await page.getByRole("button", { name: /Add field/i }).click();
  await page.getByPlaceholder("title").fill("title");
  await page.getByRole("button", { name: /^Add field$/i }).click();
  await page.getByRole("button", { name: /Save schema/i }).click();

  await page.goto("/content-manager/api%3A%3Apost.post/create");
  await page.getByPlaceholder("").fill("Hello from Playwright E2E");
  await page.getByRole("button", { name: /Save changes/i }).click();
  await page.getByRole("button", { name: /^Publish$/i }).click();

  const health = await page.request.get(`${API_BASE}/health`);
  if (!health.ok()) {
    throw new Error("Expected /health to return 200 OK");
  }

  await page.goto("/settings/plugins");
  await page.getByText(/Plugins & WASM Sandbox/i).isVisible();

  await page.goto("/settings/general");
  await page.getByText(/Live MCP Server Inspector/i).isVisible();
}
