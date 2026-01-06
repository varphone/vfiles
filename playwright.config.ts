import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/e2e",
  timeout: 30_000,
  workers: 1,
  webServer: {
    command: "bun ./scripts/start-e2e-server.ts",
    port: 3000,
    reuseExistingServer: false,
    timeout: 30_000,
  },
  use: {
    headless: true,
    viewport: { width: 1280, height: 800 },
    actionTimeout: 5_000,
  },
  projects: [
    { name: "chromium", use: { browserName: "chromium" } },
    { name: "firefox", use: { browserName: "firefox" } },
    { name: "webkit", use: { browserName: "webkit" } },
    // Mobile emulation for mobile tests
    {
      name: "mobile-chrome",
      use: {
        browserName: "chromium",
        viewport: { width: 390, height: 844 },
        userAgent: "Mozilla/5.0 (iPhone; CPU iPhone OS 15_0 like Mac OS X)",
      },
    },
  ],
});
