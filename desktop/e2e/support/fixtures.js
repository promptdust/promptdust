// playwright-bdd wiring: a custom `pd` fixture (the app driver) plus the Given/When/Then
// bound to it. Step files import { Given, When, Then } from here.

import { test as base, createBdd } from "playwright-bdd";
import { installFakeBackend } from "./fakeBackend.js";
import { REPORT_WITH_DATA } from "./data.js";

// Drives the app under test: holds the per-scenario backend config, boots the UI with the fake
// backend injected, and models a "relaunch" as a page reload (the fake store persists).
class PdWorld {
  constructor(page) {
    this.page = page;
    // Default scenario config; Given-steps mutate this before open().
    this.config = { report: REPORT_WITH_DATA, runs: [], failing: {} };
  }

  async open() {
    await installFakeBackend(this.page, this.config);
    await this.page.goto("/");
    await this.page.waitForSelector("#app *");
  }

  async relaunch() {
    await this.page.reload();
    await this.page.waitForSelector("#app *");
  }
}

export const test = base.extend({
  pd: async ({ page }, use) => {
    await use(new PdWorld(page));
  },
});

export const { Given, When, Then } = createBdd(test);
