// Tests for the app wiring (main.js). Runs headless under `node --test`: a mocked __TAURI__
// stands in for the backend and fake elements drive `dispatch`, so every state transition and
// command call is exercised without a real webview. render() is a no-op here (no #app node).

import test, { beforeEach } from "node:test";
import assert from "node:assert/strict";

import {
  state,
  invoke,
  dispatch,
  init,
  selectScan,
  doScan,
  refreshTelemetry,
} from "./main.js";
import { renderList } from "./panel.mjs";

const RUN_A = "a".repeat(32);
const RUN_B = "b".repeat(32);

const REPORT = {
  host: { os: "macos" },
  exposure: { score: 71, band: "high" },
  assurance: { score: 88, band: "high" },
  summary: { total_findings: 3, total_bytes: 213_056_716, by_exposure: { critical: 1, high: 1, medium: 1 }, by_tool: { "Claude Code": 2, Cursor: 1 } },
  findings: [
    { tool: "Claude Code", path: "~/Dropbox/.claude/projects/api", exposure_level: "critical", size_bytes: 163_983_360, file_count: 214, amplifiers: ["cloud_sync"], why: "Transcripts.", guidance: ["Move it."] },
    { tool: "Claude Code", path: "~/.claude/history.jsonl", exposure_level: "medium", size_bytes: 2_202_009, file_count: 1, amplifiers: [], why: "History.", guidance: [] },
    { tool: "Cursor", path: "~/Cursor/state.vscdb", exposure_level: "high", size_bytes: 46_871_347, file_count: 1, amplifiers: ["world_readable"], why: "SQLite.", guidance: ["Chmod."] },
  ],
};
const INDEX = [
  { run_id: RUN_A, ran_at: "2026-07-17T14:12:00Z", exposure: { score: 71, band: "high" }, confidence: { score: 88, band: "high" }, headline: "156.4 MB · Claude Code", trace_count: 3, unread: true },
  { run_id: RUN_B, ran_at: "2026-07-16T09:03:00Z", exposure: { score: 44, band: "low" }, confidence: { score: 76, band: "partial" }, headline: "44.7 MB · Cursor", trace_count: 2, unread: false },
];
const NEW_ENTRY = { run_id: "c".repeat(32), ran_at: "2026-07-18T00:00:00Z", exposure: { score: 60, band: "moderate" }, confidence: { score: 80, band: "high" }, headline: "156.4 MB · Claude Code", trace_count: 3, unread: true };

let calls = [];
function installTauri(overrides = {}) {
  const responses = {
    list_scans: () => JSON.stringify(INDEX),
    load_scan: ({ runId }) => JSON.stringify({ run_id: runId, report: REPORT, item_state: {} }),
    mark_scan_read: () => undefined,
    set_finding_state: () => undefined,
    run_scan: () => JSON.stringify(REPORT),
    save_scan: () => JSON.stringify(NEW_ENTRY),
    telemetry_status: () => JSON.stringify({ enabled: false, suppressed_by_env: false }),
    telemetry_set_enabled: () => undefined,
    telemetry_preview: () => '{"kind":"promptdust-telemetry"}',
    diagnostics: () => '{"kind":"promptdust-diagnostics"}',
    reveal: () => undefined,
    export_report: () => "/Users/x/Downloads/promptdust-report.md",
    share: () => undefined,
    ...overrides,
  };
  globalThis.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        calls.push({ cmd, args });
        const r = responses[cmd];
        if (typeof r !== "function") throw new Error(`unexpected command ${cmd}`);
        const v = r(args);
        if (v instanceof Error) throw v;
        return v;
      },
    },
  };
}
const called = (cmd) => calls.filter((c) => c.cmd === cmd);

// Minimal browser globals so theme/init paths run headless. `navigator` is a read-only global
// in modern Node, so we leave it: detectOS falls back to "unknown" and copyText's optional
// `navigator?.clipboard?.writeText` simply short-circuits — both fine for these tests.
globalThis.window = { matchMedia: () => ({ matches: false }) };
let stored = {};
globalThis.localStorage = { getItem: (k) => stored[k] ?? null, setItem: (k, v) => { stored[k] = String(v); } };
let themeAttr = null;
const appNode = { innerHTML: "" };
globalThis.document = {
  documentElement: {
    getAttribute: () => themeAttr,
    setAttribute: (_k, v) => { themeAttr = v; },
    removeAttribute: () => { themeAttr = null; },
  },
  getElementById: (id) => (id === "app" ? appNode : null), // render() paints the app tree here
};

// A fake clicked element: closest(sel) resolves against `map` (sel → dataset).
function el(map) {
  return { closest: (sel) => (Object.hasOwn(map, sel) ? { dataset: map[sel] } : null) };
}

beforeEach(() => {
  calls = [];
  stored = {};
  themeAttr = null;
  Object.assign(state, {
    screen: "welcome", os: "unknown", index: [], selRunId: null, report: null,
    itemState: {}, filter: null, expanded: new Set(), selPath: null, inboxOpen: false,
    listMenuOpen: false, detailMenuOpen: false, shareOpen: false, infoOpen: false, pinsOpen: false,
    settingsOpen: false, consentOpen: false, telePreviewOpen: false, diagOpen: false,
    telemetry: false, suppressedByEnv: false, telePreviewText: "", diagText: "", toast: "", scanning: false, permMsg: "", updateStatus: "",
  });
  installTauri();
});

/* ---------------------------------------------------------------- boot */
test("invoke rejects outside the app", async () => {
  const saved = globalThis.__TAURI__;
  delete globalThis.__TAURI__;
  await assert.rejects(() => invoke("run_scan"));
  globalThis.__TAURI__ = saved;
});

test("init loads history and opens the newest run", async () => {
  await init();
  assert.equal(state.screen, "workspace");
  assert.equal(state.selRunId, RUN_A);
  assert.equal(state.index.length, 2);
  assert.ok(state.report);
  assert.equal(state.os, "macos"); // from the report host
  assert.ok(called("list_scans").length);
  assert.ok(called("mark_scan_read").length);
});

test("init falls back to welcome when there is no history", async () => {
  installTauri({ list_scans: () => JSON.stringify([]) });
  await init();
  assert.equal(state.screen, "welcome");
  assert.equal(state.selRunId, null);
});

test("init survives a backend error (not in app)", async () => {
  installTauri({ list_scans: () => new Error("no app") });
  await init();
  assert.equal(state.screen, "welcome");
});

test("refreshTelemetry mirrors the consent store", async () => {
  installTauri({ telemetry_status: () => JSON.stringify({ enabled: true, suppressed_by_env: true }) });
  await refreshTelemetry();
  assert.equal(state.telemetry, true);
  assert.equal(state.suppressedByEnv, true);
});

/* ---------------------------------------------------------------- scans */
test("selectScan hydrates the run, marks it read, and selects the top finding", async () => {
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  assert.equal(state.screen, "workspace");
  assert.equal(state.report.findings.length, 3);
  assert.equal(state.selPath, REPORT.findings[0].path); // critical → default selection
  assert.ok(state.expanded.has("Claude Code"));
  assert.equal(state.index.find((e) => e.run_id === RUN_A).unread, false);
  // opening the default finding persists a read
  assert.ok(called("set_finding_state").some((c) => c.args.patch.read === true));
});

test("doScan (full) runs, persists, prepends, and opens the new run", async () => {
  state.index = structuredClone(INDEX);
  await doScan({ inline: false });
  assert.ok(called("run_scan").length);
  assert.ok(called("save_scan").length);
  assert.equal(state.index[0].run_id, NEW_ENTRY.run_id, "new run prepended");
  assert.equal(state.selRunId, NEW_ENTRY.run_id);
  assert.equal(state.screen, "workspace");
});

test("doScan shows empty when the scan finds nothing", async () => {
  installTauri({ run_scan: () => JSON.stringify({ ...REPORT, findings: [], summary: { total_findings: 0, by_exposure: {} } }) });
  await doScan({ inline: false });
  assert.equal(state.screen, "empty");
  assert.equal(called("save_scan").length, 0, "nothing to save");
});

test("doScan surfaces a scan error on the permission screen", async () => {
  installTauri({ run_scan: () => new Error("Library access denied") });
  await doScan({ inline: false });
  assert.equal(state.screen, "permission");
  assert.match(state.permMsg, /denied/);
});

test("doScan ignores re-entry while already scanning", async () => {
  state.scanning = true;
  await doScan({ inline: true });
  assert.equal(called("run_scan").length, 0);
});

/* ---------------------------------------------------------------- dispatch routing */
test("dispatch: inbox toggle, run select, group expand, finding select", async () => {
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  calls = [];

  dispatch(el({ "[data-inbox-toggle]": {} }));
  assert.equal(state.inboxOpen, true);

  await dispatch(el({ "[data-run]": { run: RUN_B } }));
  assert.equal(state.selRunId, RUN_B);

  const before = state.expanded.has("Cursor");
  dispatch(el({ "[data-group]": { group: "Cursor" } }));
  assert.notEqual(state.expanded.has("Cursor"), before);

  await dispatch(el({ "[data-find]": { find: "2" } }));
  assert.equal(state.selPath, REPORT.findings[2].path);
});

test("dispatch: filter capsule toggles on and off", async () => {
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  dispatch(el({ "[data-flevel]": { flevel: "high" } }));
  assert.equal(state.filter, "high");
  dispatch(el({ "[data-flevel]": { flevel: "high" } }));
  assert.equal(state.filter, null);
});

test("dispatch: list menu actions (markread / expand / collapse / export)", async () => {
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  calls = [];

  await dispatch(el({ "[data-listaction]": { listaction: "markread" } }));
  assert.ok(Object.values(state.itemState).every((s) => s.read));
  assert.equal(state.index.find((e) => e.run_id === RUN_A).unread, false);

  await dispatch(el({ "[data-listaction]": { listaction: "expand" } }));
  assert.ok(state.expanded.has("Claude Code") && state.expanded.has("Cursor"));

  await dispatch(el({ "[data-listaction]": { listaction: "collapse" } }));
  assert.equal(state.expanded.size, 0);

  await dispatch(el({ "[data-listaction]": { listaction: "export" } }));
  assert.ok(called("export_report").length);
});

test("dispatch: detail actions pin/flag/unread/copy persist and toast", async () => {
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  const path = state.selPath;

  await dispatch(el({ "[data-detailaction]": { detailaction: "pin" } }));
  assert.equal(state.itemState[path].pinned, true);

  await dispatch(el({ "[data-detailaction]": { detailaction: "flag" } }));
  assert.equal(state.itemState[path].flagged, true);

  await dispatch(el({ "[data-detailaction]": { detailaction: "unread" } }));
  assert.equal(state.itemState[path].read, false);

  await dispatch(el({ "[data-detailaction]": { detailaction: "copy" } }));
  assert.equal(state.toast, "Path copied");
});

test("dispatch: share via native sheet, and clipboard fallback when it errors", async () => {
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  await dispatch(el({ "[data-share]": { share: "native" } }));
  assert.ok(called("share").length);

  installTauri({ share: () => new Error("macOS only") });
  calls = [];
  await dispatch(el({ "[data-share]": { share: "native" } }));
  assert.equal(state.toast, "Summary copied", "falls back to clipboard");

  await dispatch(el({ "[data-share]": { share: "copy" } }));
  assert.equal(state.toast, "Summary copied");
});

test("dispatch: reveal invokes the read-only reveal command", async () => {
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  calls = [];
  await dispatch(el({ "[data-reveal]": {} }));
  assert.equal(called("reveal")[0].args.path, state.selPath);
});

test("dispatch: settings, info popover, theme toggle, and menu toggles", async () => {
  dispatch(el({ "[data-settings-toggle]": {} }));
  assert.equal(state.settingsOpen, true);
  dispatch(el({ "[data-info-toggle]": {}, "[data-menu]": {} }));
  assert.equal(state.infoOpen, true);
  dispatch(el({ "[data-theme-toggle]": {} }));
  assert.equal(themeAttr, "dark"); // from light default → dark
  assert.equal(stored["promptdust-theme"], "dark");

  dispatch(el({ "[data-listmenu-toggle]": {}, "[data-menu]": {} }));
  assert.equal(state.listMenuOpen, true);
  dispatch(el({ "[data-detailmenu-toggle]": {}, "[data-menu]": {} }));
  assert.equal(state.detailMenuOpen, true);
  dispatch(el({ "[data-share-toggle]": {}, "[data-menu]": {} }));
  assert.equal(state.shareOpen, true);
});

test("dispatch: pins toggle + outside click closes menus", () => {
  state.pinsOpen = true;
  // a click inside a menu does NOT close it
  dispatch(el({ "[data-menu]": {} }));
  assert.equal(state.pinsOpen, true);
  // a click outside any menu closes open menus
  dispatch(el({}));
  assert.equal(state.pinsOpen, false);
  // toggling pins on
  dispatch(el({ "[data-pins-toggle]": {}, "[data-menu]": {} }));
  assert.equal(state.pinsOpen, true);
});

test("dispatch: consent + telemetry + previews + diagnostics", async () => {
  await dispatch(el({ "[data-open-consent]": {} }));
  assert.equal(state.consentOpen, true);
  await dispatch(el({ "[data-consent]": { consent: "yes" } }));
  assert.equal(state.telemetry, true);
  assert.equal(state.consentOpen, false);

  await dispatch(el({ "[data-tele-toggle]": {} }));
  assert.equal(state.telemetry, false);

  await dispatch(el({ "[data-tele-preview]": {} }));
  assert.ok(state.telePreviewOpen);
  assert.match(state.telePreviewText, /promptdust-telemetry/);

  await dispatch(el({ "[data-diag-open]": {} }));
  assert.ok(state.diagOpen);
  assert.match(state.diagText, /promptdust-diagnostics/);
  await dispatch(el({ "[data-diag-share]": {} }));
  assert.ok(called("share").length);
});

test("dispatch: scan entry points and close-overlay", async () => {
  await dispatch(el({ "[data-scan-start]": {} }));
  assert.ok(called("run_scan").length);

  state.settingsOpen = true;
  dispatch(el({ "[data-close-ov]": {} }));
  assert.equal(state.settingsOpen, false);

  state.screen = "workspace";
  dispatch(el({ "[data-go-welcome]": {} }));
  assert.equal(state.screen, "welcome");
});

test("dispatch: null element and unknown click are no-ops", () => {
  assert.equal(dispatch(null), undefined);
  assert.equal(dispatch(el({})), undefined);
});

test("render() paints the app tree into #app", async () => {
  await init();
  assert.ok(appNode.innerHTML.includes("Exposure · high"), "workspace was painted");
});

test("doScan inline prepends a run without leaving the workspace", async () => {
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  state.screen = "workspace";
  await doScan({ inline: true });
  assert.equal(state.index[0].run_id, NEW_ENTRY.run_id);
  assert.equal(state.screen, "workspace");
});

test("backend errors are swallowed (persistence/telemetry) or surfaced (reveal)", async () => {
  // telemetry_status failure keeps defaults
  installTauri({ telemetry_status: () => new Error("x") });
  await refreshTelemetry();
  assert.equal(state.telemetry, false);
  assert.equal(state.suppressedByEnv, false);

  // selectScan tolerates a mark_scan_read failure
  installTauri({ mark_scan_read: () => new Error("nope") });
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  assert.equal(state.screen, "workspace");

  // set_finding_state failure: the in-memory state still reflects the change
  installTauri({ set_finding_state: () => new Error("io") });
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  const path = state.selPath;
  await dispatch(el({ "[data-detailaction]": { detailaction: "pin" } }));
  assert.equal(state.itemState[path].pinned, true);

  // export failure → the error surfaces in a toast
  installTauri({ export_report: () => new Error("disk full") });
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  await dispatch(el({ "[data-listaction]": { listaction: "export" } }));
  assert.match(state.toast, /disk full/);

  // reveal failure → the permission screen (surfaced, not swallowed)
  installTauri({ reveal: () => new Error("access denied") });
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  await dispatch(el({ "[data-reveal]": {} }));
  assert.equal(state.screen, "permission");
  assert.match(state.permMsg, /denied/);
});

test("consent / telemetry / preview / diagnostics error paths", async () => {
  // consent still closes even if the write fails
  installTauri({ telemetry_set_enabled: () => new Error("boom") });
  state.consentOpen = true;
  await dispatch(el({ "[data-consent]": { consent: "yes" } }));
  assert.equal(state.consentOpen, false);

  // telemetry toggle failure → error toast
  await dispatch(el({ "[data-tele-toggle]": {} }));
  assert.match(state.toast, /boom/);

  // preview / diagnostics failures land in the overlay body
  installTauri({ telemetry_preview: () => new Error("no preview") });
  await dispatch(el({ "[data-tele-preview]": {} }));
  assert.match(state.telePreviewText, /no preview/);

  installTauri({ diagnostics: () => new Error("no diag") });
  await dispatch(el({ "[data-diag-open]": {} }));
  assert.match(state.diagText, /no diag/);

  // shareDiag with nothing to share is a no-op
  installTauri();
  state.diagText = "";
  calls = [];
  await dispatch(el({ "[data-diag-share]": {} }));
  assert.equal(called("share").length, 0);

  // shareDiag when the sheet errors → clipboard fallback toast
  installTauri({ share: () => new Error("macOS only") });
  state.diagText = "bundle";
  await dispatch(el({ "[data-diag-share]": {} }));
  assert.match(state.toast, /Diagnostics copied/);
});

test("selectFinding ignores an out-of-range index", async () => {
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  const before = state.selPath;
  await dispatch(el({ "[data-find]": { find: "999" } }));
  assert.equal(state.selPath, before, "no such finding → selection unchanged");
});

test("dispatch: check for updates (unavailable / up-to-date / installs / error)", async () => {
  // no updater API in this build
  await dispatch(el({ "[data-check-updates]": {} }));
  assert.match(state.updateStatus, /aren't available/);

  // up to date
  globalThis.__TAURI__.updater = { check: async () => null };
  await dispatch(el({ "[data-check-updates]": {} }));
  assert.match(state.updateStatus, /latest version/);

  // an update is available → verified download + in-place install (no full re-download)
  let installed = false;
  globalThis.__TAURI__.updater = {
    check: async () => ({ version: "0.3.1", downloadAndInstall: async () => { installed = true; } }),
  };
  await dispatch(el({ "[data-check-updates]": {} }));
  assert.ok(installed, "downloadAndInstall ran");
  assert.match(state.updateStatus, /Updated to 0\.3\.1/);

  // check() failure is surfaced, not thrown
  globalThis.__TAURI__.updater = { check: async () => { throw new Error("network down"); } };
  await dispatch(el({ "[data-check-updates]": {} }));
  assert.match(state.updateStatus, /Update check failed/);
});

test("integration: clicking a filtered row selects the finding it shows", async () => {
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  await dispatch(el({ "[data-flevel]": { flevel: "high" } })); // apply filter (auto-expands the matching group)
  // render the list exactly as the app does, take the shown row's data-find, and click it
  const m = renderList(state.report, state).match(/data-find="(\d+)"[^>]*data-file="([^"]*)"/);
  assert.ok(m, "a filtered row rendered");
  await dispatch(el({ "[data-find]": { find: m[1] } }));
  assert.equal(state.selPath.split("/").pop(), m[2], "selected the finding whose row was clicked");
});

test("dispatch: a tool can be collapsed even while a filter is active (regression)", async () => {
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  await dispatch(el({ "[data-flevel]": { flevel: "high" } })); // filter high -> Cursor auto-expanded
  assert.ok(state.expanded.has("Cursor"), "applying a filter expands the matching group");
  assert.match(renderList(state.report, state), /data-file="state\.vscdb"/, "matching finding visible");
  await dispatch(el({ "[data-group]": { group: "Cursor" } })); // click the header to collapse
  assert.ok(!state.expanded.has("Cursor"), "the header collapses the group even while filtered");
  assert.ok(!/data-file="state\.vscdb"/.test(renderList(state.report, state)), "its rows hide after collapse");
});

test("copyText tolerates a failing clipboard (catch is swallowed)", async () => {
  let defined = false;
  try {
    Object.defineProperty(globalThis.navigator, "clipboard", {
      value: { writeText: async () => { throw new Error("no clipboard"); } },
      configurable: true,
    });
    defined = true;
  } catch {
    /* navigator is locked down in this runtime — nothing to exercise */
  }
  if (!defined) return;
  state.index = structuredClone(INDEX);
  await selectScan(RUN_A);
  await dispatch(el({ "[data-detailaction]": { detailaction: "copy" } }));
  assert.equal(state.toast, "Path copied", "the toast still fires despite the clipboard error");
});
