import { defineConfig, devices } from "@playwright/test";
import { defineBddConfig } from "playwright-bdd";

// Generate Playwright tests from the .feature files + step definitions.
const testDir = defineBddConfig({
  features: "features/**/*.feature",
  // Include the fixtures file (it exports the custom `test` from createBdd) plus the steps.
  steps: ["support/fixtures.js", "steps/**/*.js"],
});

const PORT = Number(process.env.PD_E2E_PORT) || 5599;

export default defineConfig({
  testDir,
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : [["list"]],
  use: {
    baseURL: `http://localhost:${PORT}`,
    trace: "on-first-retry",
    screenshot: "only-on-failure",
  },
  // WebKit matches the macOS WKWebView family the shipped app runs in.
  projects: [{ name: "webkit", use: { ...devices["Desktop Safari"] } }],
  webServer: {
    command: "node serve.mjs",
    url: `http://localhost:${PORT}`,
    reuseExistingServer: !process.env.CI,
    stdout: "ignore",
    stderr: "pipe",
  },
});
