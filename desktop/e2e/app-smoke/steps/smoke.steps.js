// Steps for the real-app smoke suite. `$`, `$$`, `browser`, and `expect` are WebdriverIO
// globals injected by the runner. Each scenario starts from a wiped config dir (fresh welcome).
import { Before, When, Then } from "@cucumber/cucumber";
import { rm } from "node:fs/promises";

// Fresh state per scenario: clear the throwaway store and relaunch the app → welcome screen.
Before(async () => {
  if (process.env.PROMPTDUST_CONFIG_DIR) {
    await rm(process.env.PROMPTDUST_CONFIG_DIR, { recursive: true, force: true });
  }
  await browser.reloadSession();
});

When("I run a scan", async () => {
  const scanBtn = await $("button*=Scan");
  await scanBtn.waitForClickable();
  await scanBtn.click();
  await $('[data-testid="score-exposure"]').waitForExist({ timeout: 30000 });
});

When("I restart the app", async () => {
  // reloadSession relaunches the app; PROMPTDUST_CONFIG_DIR persists, so history is retained.
  await browser.reloadSession();
  await $("#app *").waitForExist();
});

Then("the workspace shows an Exposure score", async () => {
  await expect($('[data-testid="score-exposure"]')).toBeExisting();
});

Then("at least one finding is listed", async () => {
  const rows = await $$("[data-file]");
  await expect(rows.length).toBeGreaterThan(0);
});

Then("the Inbox has at least one run", async () => {
  const toggle = await $("[data-inbox-toggle]");
  await toggle.waitForClickable();
  await toggle.click();
  const runs = await $$("[data-run]");
  await expect(runs.length).toBeGreaterThan(0);
});
