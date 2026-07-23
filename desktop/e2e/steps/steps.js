import { expect } from "@playwright/test";
import { Given, When, Then } from "../support/fixtures.js";
import { REPORT_WITH_DATA, REPORT_EMPTY, SEEDED_RUNS } from "../support/data.js";

/* -------------------------------------------------- Given: scenario setup */
Given("a machine with AI-tool data present", ({ pd }) => {
  pd.config.report = REPORT_WITH_DATA;
});
Given("a machine with no AI-tool data", ({ pd }) => {
  pd.config.report = REPORT_EMPTY;
});
Given("no previous scans", ({ pd }) => {
  pd.config.runs = [];
});
Given("{int} previous scans", ({ pd }, n) => {
  pd.config.runs = SEEDED_RUNS.slice(0, n);
});
Given("the scan fails with {string}", ({ pd }, msg) => {
  pd.config.failing = { ...pd.config.failing, run_scan: msg };
});
Given("revealing a file will fail with {string}", ({ pd }, msg) => {
  pd.config.failing = { ...pd.config.failing, reveal: msg };
});
Given("the native share sheet is unavailable", ({ pd }) => {
  pd.config.shareFails = true;
});
Given("anonymous sharing is on", ({ pd }) => {
  pd.config.telemetryEnabled = true;
});
Given("anonymous sharing is forced off by the environment", ({ pd }) => {
  pd.config.telemetrySuppressed = true;
});

/* -------------------------------------------------- When: actions */
When("I open PromptDust", async ({ pd }) => {
  await pd.open();
});
When("I relaunch the app", async ({ pd }) => {
  await pd.relaunch();
});
When("I click {string}", async ({ page }, label) => {
  await page.getByRole("button", { name: label }).click();
});
When("I open the Inbox", async ({ page }) => {
  await page.locator("[data-inbox-toggle]").first().click();
});
When("I select the run headlined {string}", async ({ page }, headline) => {
  await page.locator("[data-run]", { hasText: headline }).click();
});
When("I expand the {string} group", async ({ page }, tool) => {
  await page.locator(`[data-group="${tool}"]`).click();
});
When("I click the {string} tool header", async ({ page }, tool) => {
  await page.locator(`[data-group="${tool}"]`).click();
});
When("I select the finding {string}", async ({ page }, file) => {
  await page.locator(`[data-file="${file}"]`).click();
});
When("I filter by {string}", async ({ page }, level) => {
  await page.locator(`[data-flevel="${level}"]`).click();
});
When("I open the finding menu", async ({ page }) => {
  await page.locator("[data-detailmenu-toggle]").click();
});
When("I open the list menu", async ({ page }) => {
  await page.locator("[data-listmenu-toggle]").click();
});
When("I open Settings", async ({ page }) => {
  await page.locator("[data-settings-toggle]").click();
});
When("I open the share menu", async ({ page }) => {
  await page.locator("[data-share-toggle]").click();
});

/* -------------------------------------------------- Then: assertions */
Then("I see the workspace", async ({ page }) => {
  await expect(page.getByTestId("score-exposure")).toBeVisible();
});
Then("the Exposure score is shown", async ({ page }) => {
  await expect(page.getByTestId("score-exposure")).toContainText("Exposure");
});
Then("the Confidence score is shown", async ({ page }) => {
  await expect(page.getByTestId("score-confidence")).toContainText("Confidence");
});
Then("I see {string}", async ({ page }, text) => {
  await expect(page.getByText(text, { exact: false }).first()).toBeVisible();
});
Then("I do not see {string}", async ({ page }, text) => {
  await expect(page.getByText(text, { exact: false })).toHaveCount(0);
});
Then("I do not see the word {string}", async ({ page }, word) => {
  const text = (await page.locator("#app").innerText()).toLowerCase();
  expect(text).not.toContain(word.toLowerCase());
});
Then("I see {int} run(s) in the Inbox", async ({ page }, n) => {
  await expect(page.locator("[data-run]")).toHaveCount(n);
});
Then("the finding {string} is pinned", async ({ page }, file) => {
  await expect(page.locator(`[data-file="${file}"]`)).toHaveAttribute("data-pinned", "true");
});
Then("the finding {string} is not pinned", async ({ page }, file) => {
  await expect(page.locator(`[data-file="${file}"]`)).toHaveAttribute("data-pinned", "false");
});
Then("the finding {string} is read", async ({ page }, file) => {
  await expect(page.locator(`[data-file="${file}"]`)).toHaveAttribute("data-read", "true");
});
Then("a toast says {string}", async ({ page }, text) => {
  await expect(page.getByTestId("toast")).toContainText(text);
});
Then("the reveal command ran", async ({ page }) => {
  await expect.poll(() => page.evaluate(() => globalThis.__pd_revealed === true)).toBe(true);
});
Then("the share command ran", async ({ page }) => {
  await expect.poll(() => page.evaluate(() => globalThis.__pd_shared === true)).toBe(true);
});
