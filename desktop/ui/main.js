// DOM wiring for the PromptDust desktop app (Panel + Inbox redesign). All data→HTML lives in
// the tested render.mjs + panel.mjs; this file owns state, events, and the Tauri commands, and
// re-renders the whole app from renderApp(state) on every change (the prototype's model).
// No network, no content — ever.
//
// Structured for testability: the state transitions live in `dispatch(el)` and the async
// command orchestration functions, all exercised in main.test.mjs with a mocked __TAURI__ and
// fake elements. `render()` is a no-op without a DOM, and the DOM/event binding at the bottom
// only registers when a document exists, so the module imports cleanly under `node --test`.

import { hasFindings, reportToMarkdown, detectOS, humanSize } from "./render.mjs";
import { renderApp, groupsOf } from "./panel.mjs";

// Invoke a Tauri command. Reads __TAURI__ at call time so tests can inject a mock.
export function invoke(cmd, args) {
  const tauri = globalThis.__TAURI__;
  if (!tauri) return Promise.reject(new Error("This page must run inside the PromptDust app."));
  return tauri.core.invoke(cmd, args);
}

export const state = {
  screen: "welcome", // welcome | scanning | workspace | empty | permission
  os: "unknown",
  index: [], // list_scans: newest first
  selRunId: null,
  report: null,
  itemState: {}, // path -> { read, pinned, flagged } for the selected run
  filter: null,
  expanded: new Set(), // expanded tool groups in the selected run
  selPath: null,
  inboxOpen: false,
  listMenuOpen: false,
  detailMenuOpen: false,
  shareOpen: false,
  infoOpen: false,
  pinsOpen: false,
  settingsOpen: false,
  consentOpen: false,
  telePreviewOpen: false,
  diagOpen: false,
  telemetry: false,
  suppressedByEnv: false,
  telePreviewText: "",
  diagText: "",
  toast: "",
  scanning: false,
  permMsg: "",
  updateStatus: "",
};

export function render() {
  if (typeof document === "undefined") return; // node/test: no DOM to paint
  const el = document.getElementById("app");
  if (el) el.innerHTML = renderApp(state);
}

let toastTimer;
function toast(msg) {
  clearTimeout(toastTimer);
  state.toast = msg;
  closeMenus();
  render();
  toastTimer = setTimeout(() => {
    state.toast = "";
    render();
  }, 2600);
  // Don't let a pending toast keep a headless process (tests) alive.
  if (typeof toastTimer?.unref === "function") toastTimer.unref();
}

function closeMenus() {
  state.listMenuOpen = false;
  state.detailMenuOpen = false;
  state.shareOpen = false;
  state.infoOpen = false;
  state.pinsOpen = false;
}
function closeOverlays() {
  state.settingsOpen = false;
  state.consentOpen = false;
  state.telePreviewOpen = false;
  state.diagOpen = false;
}

/* ---------------------------------------------------------------- theme */
function applyTheme(theme) {
  const root = document.documentElement;
  if (theme === "dark" || theme === "light") root.setAttribute("data-theme", theme);
  else root.removeAttribute("data-theme");
}
function toggleTheme() {
  const cur = document.documentElement.getAttribute("data-theme");
  const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const effective = cur || (prefersDark ? "dark" : "light");
  const next = effective === "dark" ? "light" : "dark";
  applyTheme(next);
  localStorage.setItem("promptdust-theme", next);
  render();
}

/* ---------------------------------------------------------------- helpers */
function showError(err) {
  state.scanning = false;
  state.permMsg = String(err?.message ?? err);
  state.screen = "permission";
  render();
}

async function copyText(t) {
  try {
    await globalThis.navigator?.clipboard?.writeText(t);
  } catch {
    /* clipboard unavailable — the toast still confirms intent */
  }
}

function shareSummary(f) {
  return `${f.tool} · ${f.path} · ${humanSize(f.size_bytes)} · ${f.exposure_level}`;
}

/* ---------------------------------------------------------------- data flow */
export async function refreshTelemetry() {
  try {
    const s = JSON.parse(await invoke("telemetry_status"));
    state.telemetry = !!s.enabled;
    state.suppressedByEnv = !!s.suppressed_by_env;
  } catch {
    /* not in the app / no consent store yet — keep defaults */
  }
}

export async function selectScan(runId) {
  const run = JSON.parse(await invoke("load_scan", { runId }));
  state.report = run.report;
  state.itemState = run.item_state ?? {};
  state.selRunId = runId;
  state.os = state.report.host?.os ?? state.os;
  state.filter = null;
  const groups = groupsOf(state.report.findings ?? []);
  state.expanded = new Set(groups.length ? [groups[0][0]] : []);
  state.selPath = groups.length ? groups[0][1][0].f.path : null;
  state.screen = "workspace";
  closeMenus();
  try {
    await invoke("mark_scan_read", { runId });
  } catch {
    /* best effort */
  }
  const entry = state.index.find((e) => e.run_id === runId);
  if (entry) entry.unread = false;
  if (state.selPath) await persistFinding(runId, state.selPath, { read: true });
  render();
}

export async function doScan({ inline }) {
  if (state.scanning) return;
  try {
    if (inline) state.scanning = true;
    else state.screen = "scanning";
    render();
    const json = await invoke("run_scan", { noSlow: false, mode: "inventory" });
    const report = JSON.parse(json);
    if (!hasFindings(report)) {
      state.scanning = false;
      state.screen = "empty";
      return render();
    }
    const entry = JSON.parse(await invoke("save_scan", { reportJson: json }));
    state.index.unshift(entry);
    state.scanning = false;
    await selectScan(entry.run_id);
  } catch (err) {
    showError(err);
  }
}

async function persistFinding(runId, path, patch) {
  const st = state.itemState[path] ?? { read: false, pinned: false, flagged: false };
  Object.assign(st, patch);
  state.itemState[path] = st;
  try {
    await invoke("set_finding_state", { runId, path, patch });
  } catch {
    /* local cache still reflects it; a later save reconciles */
  }
}

async function selectFinding(idx) {
  const f = state.report?.findings?.[idx];
  if (!f) return;
  state.selPath = f.path;
  closeMenus();
  await persistFinding(state.selRunId, f.path, { read: true });
  render();
}

async function listAction(a) {
  if (a === "markread") {
    for (const f of state.report?.findings ?? []) await persistFinding(state.selRunId, f.path, { read: true });
    const entry = state.index.find((e) => e.run_id === state.selRunId);
    if (entry) entry.unread = false;
    toast("All items marked as read");
  } else if (a === "expand") {
    state.expanded = new Set((state.report?.findings ?? []).map((f) => f.tool));
    state.listMenuOpen = false;
    render();
  } else if (a === "collapse") {
    state.expanded.clear();
    state.listMenuOpen = false;
    render();
  } else if (a === "export") {
    state.listMenuOpen = false;
    await exportReport();
  }
}

async function exportReport() {
  try {
    const saved = await invoke("export_report", {
      contents: reportToMarkdown(state.report),
      extension: "md",
    });
    toast(`Saved to ${saved}`);
  } catch (err) {
    toast(String(err?.message ?? err));
  }
}

async function detailAction(a) {
  const path = state.selPath;
  if (!path) return;
  const st = state.itemState[path] ?? {};
  if (a === "unread") {
    await persistFinding(state.selRunId, path, { read: false });
    toast("Marked as unread");
  } else if (a === "pin") {
    const v = !st.pinned;
    await persistFinding(state.selRunId, path, { pinned: v });
    toast(v ? "Pinned" : "Unpinned");
  } else if (a === "flag") {
    const v = !st.flagged;
    await persistFinding(state.selRunId, path, { flagged: v });
    toast(v ? "Flagged" : "Unflagged");
  } else if (a === "copy") {
    await copyText(path);
    toast("Path copied");
  }
}

async function shareAction(a) {
  const f = state.report?.findings?.find((x) => x.path === state.selPath);
  if (!f) return;
  const summary = shareSummary(f);
  if (a === "copy") {
    await copyText(summary);
    return toast("Summary copied");
  }
  try {
    await invoke("share", { text: summary });
    state.shareOpen = false;
    render();
  } catch {
    // Share sheet is macOS-only; elsewhere fall back to the clipboard.
    await copyText(summary);
    toast("Summary copied");
  }
}

async function revealSelected() {
  if (!state.selPath) return;
  try {
    await invoke("reveal", { path: state.selPath });
  } catch (err) {
    showError(err);
  }
}

async function setConsent(yes) {
  try {
    await invoke("telemetry_set_enabled", { enabled: yes });
    state.telemetry = yes;
  } catch {
    /* leave state; the toast reflects intent */
  }
  state.consentOpen = false;
  toast(yes ? "Anonymous sharing on — thank you" : "Sharing stays off");
}

async function toggleTelemetry() {
  const v = !state.telemetry;
  try {
    await invoke("telemetry_set_enabled", { enabled: v });
    state.telemetry = v;
    toast(v ? "Anonymous sharing on" : "Sharing off");
  } catch (err) {
    toast(String(err?.message ?? err));
  }
}

async function openTelePreview() {
  state.telePreviewOpen = true;
  state.telePreviewText = "";
  render();
  try {
    state.telePreviewText = await invoke("telemetry_preview", { noSlow: false });
  } catch (err) {
    state.telePreviewText = String(err?.message ?? err);
  }
  render();
}

async function openDiag() {
  state.diagOpen = true;
  state.settingsOpen = false;
  state.diagText = "";
  render();
  try {
    state.diagText = await invoke("diagnostics", { noSlow: false });
  } catch (err) {
    state.diagText = String(err?.message ?? err);
  }
  render();
}

async function shareDiag() {
  if (!state.diagText) return;
  try {
    await invoke("share", { text: state.diagText });
    toast("Diagnostics shared");
  } catch {
    await copyText(state.diagText);
    toast("Diagnostics copied");
  }
}

// Opt-in, signed self-update (Q-03): runs only on this click, never in the background. The
// updater plugin verifies the download against the configured pubkey before install. Inert
// until a release publishes an update feed (createUpdaterArtifacts + signing key, see #7);
// until then this reports "latest" / "not available".
async function checkForUpdates() {
  const updater = globalThis.__TAURI__?.updater;
  if (!updater?.check) {
    state.updateStatus = "Updates aren't available in this build.";
    return render();
  }
  state.updateStatus = "Checking…";
  render();
  try {
    const update = await updater.check();
    if (!update) {
      state.updateStatus = "You're on the latest version.";
      return render();
    }
    state.updateStatus = `Update ${update.version} available, downloading…`;
    render();
    await update.downloadAndInstall();
    state.updateStatus = `Updated to ${update.version}. Restart PromptDust to apply.`;
  } catch (err) {
    state.updateStatus = `Update check failed: ${String(err?.message ?? err)}`;
  }
  render();
}

/* ---------------------------------------------------------------- click dispatch */
// One delegated handler for the whole app. `el` is the clicked element (or a test fake with
// `closest`/`dataset`). Returns the (possibly async) effect; callers re-render as needed.
export function dispatch(el) {
  if (!el) return undefined;

  if (el.closest("[data-close-ov]")) {
    closeOverlays();
    return render();
  }

  if (el.closest("[data-scan-start]")) return doScan({ inline: false });
  if (el.closest("[data-newscan]")) return doScan({ inline: true });
  if (el.closest("[data-go-welcome]")) {
    state.screen = "welcome";
    return render();
  }

  if (el.closest("[data-inbox-toggle]")) {
    state.inboxOpen = !state.inboxOpen;
    return render();
  }
  const runEl = el.closest("[data-run]");
  if (runEl) return selectScan(runEl.dataset.run);

  const grpEl = el.closest("[data-group]");
  if (grpEl) {
    const tool = grpEl.dataset.group;
    if (state.expanded.has(tool)) state.expanded.delete(tool);
    else state.expanded.add(tool);
    state.listMenuOpen = false;
    return render();
  }

  const findEl = el.closest("[data-find]");
  if (findEl) return selectFinding(Number(findEl.dataset.find));

  if (el.closest("[data-listmenu-toggle]")) {
    state.listMenuOpen = !state.listMenuOpen;
    state.detailMenuOpen = state.shareOpen = state.infoOpen = state.pinsOpen = false;
    return render();
  }
  const la = el.closest("[data-listaction]");
  if (la) return listAction(la.dataset.listaction);

  if (el.closest("[data-pins-toggle]")) {
    state.pinsOpen = !state.pinsOpen;
    state.listMenuOpen = state.detailMenuOpen = state.shareOpen = false;
    return render();
  }

  const fl = el.closest("[data-flevel]");
  if (fl) {
    const lvl = fl.dataset.flevel || null;
    state.filter = state.filter === lvl ? null : lvl;
    // Applying a filter expands the groups that hold a matching finding, so they're visible.
    // `expanded` stays the single source of truth, so a tool can still be collapsed while a
    // filter is active (the render no longer force-opens groups).
    if (state.filter) {
      for (const f of state.report?.findings ?? []) {
        if (f.exposure_level === state.filter) state.expanded.add(f.tool);
      }
    }
    state.pinsOpen = state.listMenuOpen = false;
    return render();
  }

  if (el.closest("[data-detailmenu-toggle]")) {
    state.detailMenuOpen = !state.detailMenuOpen;
    state.listMenuOpen = state.shareOpen = state.infoOpen = false;
    return render();
  }
  const da = el.closest("[data-detailaction]");
  if (da) return detailAction(da.dataset.detailaction);

  if (el.closest("[data-share-toggle]")) {
    state.shareOpen = !state.shareOpen;
    state.detailMenuOpen = state.listMenuOpen = false;
    return render();
  }
  const sh = el.closest("[data-share]");
  if (sh) return shareAction(sh.dataset.share);
  if (el.closest("[data-reveal]")) return revealSelected();

  if (el.closest("[data-settings-toggle]")) {
    state.settingsOpen = true;
    state.infoOpen = false;
    return render();
  }
  if (el.closest("[data-info-toggle]")) {
    state.infoOpen = !state.infoOpen;
    state.detailMenuOpen = state.listMenuOpen = state.shareOpen = false;
    return render();
  }
  if (el.closest("[data-theme-toggle]")) return toggleTheme();

  if (el.closest("[data-open-consent]")) {
    state.consentOpen = true;
    return render();
  }
  const cs = el.closest("[data-consent]");
  if (cs) return setConsent(cs.dataset.consent === "yes");
  if (el.closest("[data-tele-toggle]")) return toggleTelemetry();
  if (el.closest("[data-tele-preview]")) return openTelePreview();
  if (el.closest("[data-diag-open]")) return openDiag();
  if (el.closest("[data-diag-share]")) return shareDiag();
  if (el.closest("[data-check-updates]")) return checkForUpdates();

  // A click outside any menu closes open menus/popovers.
  if (!el.closest("[data-menu]")) {
    if (state.listMenuOpen || state.detailMenuOpen || state.shareOpen || state.infoOpen || state.pinsOpen) {
      closeMenus();
      return render();
    }
  }
  return undefined;
}

function onClick(e) {
  const el = e.target instanceof Element ? e.target : null;
  dispatch(el);
}

/* ---------------------------------------------------------------- boot */
// Load history + telemetry status, then show the newest run (or welcome on first launch).
export async function init() {
  const saved = typeof localStorage !== "undefined" ? localStorage.getItem("promptdust-theme") : null;
  if (saved) applyTheme(saved);
  state.os = detectOS(globalThis.navigator?.userAgent);
  await refreshTelemetry();
  try {
    const index = JSON.parse(await invoke("list_scans"));
    state.index = index;
    if (index.length) return selectScan(index[0].run_id);
  } catch {
    /* not in app / empty history → welcome */
  }
  render();
}

if (typeof document !== "undefined") {
  document.addEventListener("DOMContentLoaded", () => {
    document.addEventListener("click", onClick);
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        closeOverlays();
        closeMenus();
        render();
      }
    });
    init();
  });
}
