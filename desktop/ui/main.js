// DOM wiring for the PromptDust desktop app. All data→HTML lives in the tested
// render.mjs; this file only handles state, events, and the three read-only Tauri
// commands (run_scan / reveal / export_report). No network, no content — ever.

import {
  renderSummaryScreen,
  renderResults,
  renderDetail,
  reportToMarkdown,
  hasFindings,
  osTerms,
  detectOS,
  renderRingOnePreview,
} from "./render.mjs";

const invoke = (cmd, args) => {
  const tauri = globalThis.__TAURI__;
  if (!tauri) return Promise.reject(new Error("This page must run inside the PromptDust app."));
  return tauri.core.invoke(cmd, args);
};

const byId = (id) => document.getElementById(id);
const SCREENS = ["welcome", "scanning", "summary", "results", "empty", "permission"];
function show(screen) {
  for (const s of SCREENS) byId(s).hidden = s !== screen;
}
function closeOverlay(which) {
  byId(`${which}-overlay`).hidden = true;
}

const state = { data: null, filter: null, format: "md" };

/* ---------- theme (in-app toggle; persists; overrides the OS default) ---------- */
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
}

/* ---------- OS-aware copy (pre-scan) ---------- */
// The report supplies host.os once a scan runs; before that, the welcome screen needs the
// device name too. Resolve it from the webview's user-agent (detectOS degrades unrecognized
// agents to neutral wording — never a wrong OS name).
function applyPreScanCopy() {
  const terms = osTerms(detectOS(navigator.userAgent));
  const scanBtn = byId("scan-btn");
  if (scanBtn) scanBtn.textContent = `Scan ${terms.device}`;
  const promise = byId("promise-device");
  if (promise) promise.textContent = `Never leaves ${terms.device}`;
  const ringPreview = byId("ring1-preview");
  if (ringPreview) ringPreview.innerHTML = renderRingOnePreview(terms.device);
}

/* ---------- flow ---------- */
async function runScan() {
  state.filter = null;
  show("scanning");
  try {
    // Ring 0 is the only ring collected today; the Ring-1 opt-in on the welcome is a disabled
    // stub. The run_scan `mode` param is the seam — when the Ring-1 collectors land, read the
    // (then-enabled) toggle here and send "usage" through it.
    const json = await invoke("run_scan", { noSlow: false, mode: "inventory" });
    state.data = JSON.parse(json);
    if (hasFindings(state.data)) {
      byId("summary-body").innerHTML = renderSummaryScreen(state.data);
      show("summary");
    } else {
      show("empty");
    }
  } catch (err) {
    byId("perm-msg").textContent = String(err?.message ?? err);
    show("permission");
  }
}

function renderResultsScreen() {
  byId("results").innerHTML = renderResults(state.data, state.filter);
  show("results");
}

function openDetail(idx) {
  const f = state.data?.findings?.[idx];
  if (!f) return;
  byId("detail-body").innerHTML = renderDetail(f, state.data?.host?.os);
  byId("detail-overlay").hidden = false;
}

function openExport() {
  byId("export-result").hidden = true;
  byId("export-overlay").hidden = false;
}

function openAbout() {
  byId("update-status").hidden = true;
  byId("about-overlay").hidden = false;
}

/* Opt-in, signed self-update (Q-03): runs only on this explicit click — never on a
   timer or at launch. The updater plugin verifies the download against the configured
   pubkey before install; here we just drive the check and report status. */
async function checkForUpdates() {
  const status = byId("update-status");
  const btn = byId("check-updates");
  status.hidden = false;
  const tauri = globalThis.__TAURI__;
  if (!tauri) {
    status.textContent = "Updates are only available in the installed app.";
    return;
  }
  const updater = tauri.updater;
  if (!updater?.check) {
    status.textContent = "Update checking isn’t available in this build.";
    return;
  }
  btn.disabled = true;
  status.textContent = "Checking…";
  try {
    const update = await updater.check();
    if (!update) {
      status.textContent = "You’re on the latest version.";
      return;
    }
    status.textContent = `Update ${update.version} available — downloading…`;
    await update.downloadAndInstall();
    status.textContent = `Updated to ${update.version}. Restart PromptDust to apply.`;
  } catch (err) {
    status.textContent = `Update check failed: ${String(err?.message ?? err)}`;
  } finally {
    btn.disabled = false;
  }
}

async function doExport() {
  if (!state.data) return;
  const contents =
    state.format === "json"
      ? JSON.stringify(state.data, null, 2)
      : reportToMarkdown(state.data);
  const el = byId("export-result");
  try {
    const saved = await invoke("export_report", { contents, extension: state.format });
    el.textContent = `Saved to ${saved}`;
  } catch (err) {
    el.textContent = String(err?.message ?? err);
  }
  el.hidden = false;
}

async function reveal(path) {
  try {
    await invoke("reveal", { path });
  } catch (err) {
    byId("perm-msg").textContent = String(err?.message ?? err);
    show("permission");
  }
}

/* ---------- one delegated click handler for the whole app ---------- */
function onClick(e) {
  const el = e.target instanceof Element ? e.target : null;
  if (!el) return;

  const closeEl = el.closest("[data-close]");
  if (closeEl) return closeOverlay(closeEl.dataset.close);

  if (el.closest("#theme-toggle")) return toggleTheme();
  if (el.closest("#about-btn")) return openAbout();
  if (el.closest("#check-updates")) return checkForUpdates();
  if (el.closest("#scan-btn") || el.closest("#perm-retry") || el.closest(".new-scan")) return runScan();
  if (el.closest("#perm-welcome")) return show("welcome");
  if (el.closest(".back-summary")) return show("summary");
  if (el.closest("#see-inventory")) {
    state.filter = null; // the full inventory is always unfiltered
    return renderResultsScreen();
  }

  const seg = el.closest(".seg-btn");
  if (seg) {
    state.format = seg.dataset.fmt;
    for (const b of document.querySelectorAll("#export-overlay .seg-btn")) {
      b.classList.toggle("active", b === seg);
    }
    return;
  }
  if (el.closest("#export-do")) return doExport();
  if (el.closest(".export-open")) return openExport();

  const rev = el.closest(".reveal");
  if (rev) return reveal(rev.dataset.path);

  if (el.closest(".show-all")) {
    state.filter = null;
    return renderResultsScreen();
  }

  const chip = el.closest(".chip");
  if (chip) {
    const lvl = chip.dataset.level || null;
    if (chip.closest("#summary-body")) {
      state.filter = lvl; // summary chip → jump into filtered results
      return renderResultsScreen();
    }
    state.filter = state.filter === lvl ? null : lvl; // results chip → toggle
    return renderResultsScreen();
  }

  const finding = el.closest(".finding");
  if (finding && finding.closest("#results")) return openDetail(Number(finding.dataset.idx));
}

document.addEventListener("DOMContentLoaded", () => {
  const saved = localStorage.getItem("promptdust-theme");
  if (saved) applyTheme(saved);
  applyPreScanCopy();
  document.addEventListener("click", onClick);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      closeOverlay("detail");
      closeOverlay("export");
      closeOverlay("about");
    }
  });
});
