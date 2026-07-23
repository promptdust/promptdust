// Tests for the Panel + Inbox pure render layer (panel.mjs). Run with `node --test`.
// Assertions target structure, content, data-* hooks, and escaping — never exact inline
// styles — so the visual port can be retuned without breaking the suite.

import test from "node:test";
import assert from "node:assert/strict";

import {
  levelOfBand,
  meter,
  tag,
  findingState,
  groupsOf,
  formatWhen,
  renderInbox,
  renderDistribution,
  renderRibbon,
  renderList,
  renderDetail,
  renderAppbar,
  renderWorkspace,
  renderToast,
  renderWelcome,
  renderScanning,
  renderEmpty,
  renderPermission,
  renderConsent,
  renderSettings,
  renderTelePreview,
  renderDiag,
  renderOverlays,
  renderApp,
} from "./panel.mjs";

const RUN_A = "a".repeat(32);
const RUN_B = "b".repeat(32);

const REPORT = {
  generated_at: "2026-07-17T14:12:00Z",
  host: { os: "macos" },
  disk_encryption: "on",
  mode: "inventory",
  exposure: { score: 71, band: "high" },
  assurance: { score: 88, band: "high" },
  summary: {
    total_findings: 3,
    total_bytes: 213_056_716,
    by_exposure: { critical: 1, high: 1, medium: 1 },
    by_tool: { "Claude Code": 2, Cursor: 1 },
  },
  findings: [
    {
      tool: "Claude Code",
      path: "~/Library/CloudStorage/Dropbox/.claude/projects/api",
      exposure_level: "critical",
      size_bytes: 163_983_360,
      file_count: 214,
      modified_epoch_secs: 1_760_000_000,
      amplifiers: ["cloud_sync", "world_readable"],
      why: "Verbatim transcripts of your sessions in a synced folder.",
      guidance: ["Move it out of the synced folder."],
    },
    {
      tool: "Claude Code",
      path: "~/.claude/history.jsonl",
      exposure_level: "medium",
      size_bytes: 2_202_009,
      file_count: 1,
      modified_epoch_secs: 1_760_000_000,
      amplifiers: [],
      why: "Your prompt history.",
      guidance: [],
    },
    {
      tool: "Cursor",
      path: "~/Library/Application Support/Cursor/state.vscdb",
      exposure_level: "high",
      size_bytes: 46_871_347,
      file_count: 1,
      modified_epoch_secs: 1_760_000_000,
      amplifiers: ["world_readable"],
      why: "Cursor's SQLite store of your chats.",
      guidance: ["Tighten the file permissions."],
    },
  ],
};

const INDEX = [
  { run_id: RUN_A, ran_at: "2026-07-17T14:12:00Z", exposure: { score: 71, band: "high" }, confidence: { score: 88, band: "high" }, headline: "156.4 MB · Claude Code", trace_count: 3, unread: true },
  { run_id: RUN_B, ran_at: "2026-07-16T09:03:00Z", exposure: { score: 44, band: "low" }, confidence: { score: 76, band: "partial" }, headline: "44.7 MB · Cursor", trace_count: 2, unread: false },
];

// A workspace UI state with everything closed and the newest run + top finding selected.
function baseUI(overrides = {}) {
  return {
    screen: "workspace",
    os: "macos",
    index: INDEX,
    report: REPORT,
    selRunId: RUN_A,
    selPath: REPORT.findings[0].path,
    filter: null,
    expanded: new Set(["Claude Code"]),
    itemState: {},
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
    ...overrides,
  };
}

/* ---------------------------------------------------------------- primitives */
test("levelOfBand maps score bands onto the gradient, unknown → info", () => {
  assert.equal(levelOfBand("critical"), "critical");
  assert.equal(levelOfBand("high"), "high");
  assert.equal(levelOfBand("moderate"), "medium");
  assert.equal(levelOfBand("low"), "low");
  assert.equal(levelOfBand("minimal"), "info");
  assert.equal(levelOfBand("nonsense"), "info");
  assert.equal(levelOfBand(undefined), "info");
});

test("meter fills up to the level and never green", () => {
  const crit = meter("critical");
  assert.equal((crit.match(/var\(--crit\)/g) || []).length, 5, "critical fills all 5 bars");
  assert.ok(!meter("info").includes("var(--crit)"));
  assert.ok(meter("info").includes("var(--info)"));
  assert.ok(!/green/i.test(meter("low")));
});

test("tag shows the display name in the level colour", () => {
  assert.ok(tag("high").includes("High"));
  assert.ok(tag("high").includes("var(--high)"));
});

test("findingState defaults to all-false and reads the backend item_state", () => {
  assert.deepEqual(findingState(undefined, "/p"), { read: false, pinned: false, flagged: false });
  assert.deepEqual(findingState({}, "/p"), { read: false, pinned: false, flagged: false });
  assert.deepEqual(
    findingState({ "/p": { read: true, pinned: true, flagged: false } }, "/p"),
    { read: true, pinned: true, flagged: false },
  );
});

test("groupsOf groups by tool, sorts within + across by highest attention, keeps original idx", () => {
  const groups = groupsOf(REPORT.findings);
  assert.equal(groups[0][0], "Claude Code", "Claude Code holds the critical → first");
  assert.equal(groups[1][0], "Cursor");
  // original indices preserved (critical finding is index 0)
  assert.equal(groups[0][1][0].idx, 0);
  assert.equal(groups[0][1][0].f.exposure_level, "critical");
  // within Claude Code, critical sorts before medium
  assert.equal(groups[0][1][1].f.exposure_level, "medium");
  assert.deepEqual(groupsOf([]), []);
});

test("formatWhen is deterministic (UTC) and survives a bad timestamp", () => {
  assert.equal(formatWhen("2026-07-17T14:12:00Z"), "Jul 17 · 2:12 PM UTC");
  assert.equal(formatWhen("2026-01-03T00:05:00Z"), "Jan 3 · 12:05 AM UTC");
  assert.equal(formatWhen("not-a-date"), "not-a-date");
});

/* ---------------------------------------------------------------- inbox */
test("renderInbox collapsed shows the count + unread indicator, no run buttons", () => {
  const html = renderInbox({ index: INDEX, selRunId: RUN_A, open: false });
  assert.ok(html.includes("data-inbox-toggle"));
  assert.ok(html.includes("Scans · 2"));
  assert.ok(!html.includes("data-run="), "collapsed rail has no run buttons");
  // one run is unread → the accent dot is present
  assert.ok(html.includes("border-radius:50%"));
  assert.ok(!renderInbox({ index: [{ ...INDEX[1] }], open: false }).match(/background:var\(--accent\)"><\/span>\s*<\/div>/));
});

test("renderInbox open lists runs with headline, traces, unread dot, and marks the selection", () => {
  const html = renderInbox({ index: INDEX, selRunId: RUN_A, open: true });
  assert.ok(html.includes(`data-run="${RUN_A}"`));
  assert.ok(html.includes(`data-run="${RUN_B}"`));
  assert.ok(html.includes("156.4 MB · Claude Code"));
  assert.ok(html.includes("Jul 17 · 2:12 PM UTC"));
  assert.ok(html.includes("3 traces"));
  assert.ok(html.includes("Exp 71 · high"));
  assert.ok(html.includes("data-newscan"));
  // selected run gets the accent left border
  const selBtn = html.slice(html.indexOf(`data-run="${RUN_A}"`), html.indexOf(`data-run="${RUN_B}"`));
  assert.ok(selBtn.includes("border-left:3px solid var(--accent)"));
});

test("renderInbox handles a missing exposure and a scanning row", () => {
  const html = renderInbox({ index: [{ run_id: RUN_A, ran_at: "2026-07-17T14:12:00Z", headline: "x", trace_count: 0 }], open: true, scanning: true });
  assert.ok(html.includes("Exp —"));
  assert.ok(html.includes("Scanning…"));
});

/* ---------------------------------------------------------------- ribbon */
test("renderDistribution renders a labelled count/bar for present levels only", () => {
  const html = renderDistribution({ critical: 1, high: 1, medium: 1 }, 3);
  assert.ok(html.includes("Distribution · 3 traces"));
  assert.ok(html.includes("Crit"));
  assert.ok(html.includes("Med"));
  assert.ok(!html.includes("Low"), "empty levels are omitted");
});

test("renderRibbon shows Exposure + Confidence (from assurance) + Distribution", () => {
  const html = renderRibbon(REPORT);
  assert.ok(html.includes("Exposure · high"));
  assert.ok(html.includes("71"));
  assert.ok(html.includes("Confidence · high"), "assurance is surfaced as Confidence");
  assert.ok(html.includes("88"));
  assert.ok(!html.includes("Assurance"), "the word Assurance never appears in the UI");
  assert.ok(html.includes("Distribution · 3 traces"));
});

test("renderRibbon degrades when scores are absent", () => {
  const html = renderRibbon({ summary: { total_findings: 0, by_exposure: {} } });
  assert.ok(html.includes("Exposure · —"));
  assert.ok(html.includes("Confidence · —"));
});

/* ---------------------------------------------------------------- list */
test("renderList collapses groups by default except the expanded one", () => {
  const html = renderList(REPORT, baseUI());
  assert.ok(html.includes('data-group="Claude Code"'));
  assert.ok(html.includes('data-group="Cursor"'));
  // Claude Code is expanded → its finding rows render; Cursor is collapsed → no row for its path
  assert.ok(html.includes("history.jsonl"));
  assert.ok(!html.includes("state.vscdb"), "collapsed group hides its rows");
});

test("renderList capsules reflect by_exposure; an expanded filtered group shows its rows", () => {
  const filtered = renderList(REPORT, baseUI({ filter: "high", expanded: new Set(["Cursor"]) }));
  assert.ok(filtered.includes('data-flevel="high"'));
  assert.ok(filtered.includes("state.vscdb"), "the expanded matching group shows its finding");
  assert.ok(!filtered.includes("history.jsonl"), "non-matching findings are filtered out");
  // a filter that matches nothing shows the empty note
  const none = renderList({ ...REPORT, findings: [] }, baseUI({ filter: "critical" }));
  assert.ok(none.includes("Nothing at this level."));
});

test("renderList shows the pins chip + popover only when something is pinned", () => {
  const path = REPORT.findings[0].path;
  const plain = renderList(REPORT, baseUI());
  assert.ok(!plain.includes("data-pins-toggle"));
  const pinned = renderList(REPORT, baseUI({ itemState: { [path]: { pinned: true } }, pinsOpen: true }));
  assert.ok(pinned.includes("data-pins-toggle"));
  assert.ok(pinned.match(/Pinned/), "the popover header renders when open");
});

test("renderList overflow menu offers read/expand/collapse + export", () => {
  const html = renderList(REPORT, baseUI({ listMenuOpen: true }));
  assert.ok(html.includes('data-listaction="markread"'));
  assert.ok(html.includes('data-listaction="expand"'));
  assert.ok(html.includes('data-listaction="collapse"'));
  assert.ok(html.includes('data-listaction="export"'));
});

test("renderList keeps ORIGINAL finding indices under a filter (regression: wrong-selection)", () => {
  // Cursor's state.vscdb is findings[2]; under filter=high it is the only visible row, but its
  // data-find must stay "2" (its original index), not the filtered position "0" — otherwise
  // selectFinding(report.findings[idx]) picks a different, filtered-out finding.
  const html = renderList(REPORT, baseUI({ filter: "high", expanded: new Set(["Cursor"]) }));
  assert.match(html, /data-find="2"[^>]*data-file="state\.vscdb"/, "filtered row keeps its original index");
  assert.ok(!/data-find="0"[^>]*data-file="state\.vscdb"/.test(html), "must not renumber to the filtered position");
});

test("renderList group unread badge counts unread findings", () => {
  const html = renderList(REPORT, baseUI({ itemState: {} }));
  // both Claude Code findings unread → badge "2" on that group header
  assert.ok(/>2<\/span>/.test(html));
});

/* ---------------------------------------------------------------- detail */
test("renderDetail shows the selected finding with amplifiers + guidance", () => {
  const html = renderDetail(REPORT, baseUI());
  assert.ok(html.includes("Claude Code"));
  assert.ok(html.includes("Verbatim transcripts"));
  assert.ok(html.includes("Cloud-synced"));
  assert.ok(html.includes("World-readable"));
  assert.ok(html.includes("Amplifying it"));
  assert.ok(html.includes("What you can do"));
  assert.ok(html.includes("Move it out of the synced folder."));
  assert.ok(html.includes("Reveal in Finder"), "macOS file-manager label");
  assert.ok(html.includes("214 files"));
});

test("renderDetail hides empty amplifier/guidance sections", () => {
  const html = renderDetail(REPORT, baseUI({ selPath: REPORT.findings[1].path }));
  assert.ok(html.includes("history.jsonl"));
  assert.ok(!html.includes("Amplifying it"));
  assert.ok(!html.includes("What you can do"));
  assert.ok(html.includes("1 file"), "singular file label");
});

test("renderDetail is an empty pane when nothing is selected, and never a delete affordance", () => {
  const html = renderDetail(REPORT, baseUI({ selPath: null }));
  assert.ok(!html.includes("data-reveal"));
  const full = renderDetail(REPORT, baseUI());
  assert.ok(!/delete/i.test(full), "no delete button anywhere");
});

test("renderDetail share + overflow menus open on state", () => {
  const share = renderDetail(REPORT, baseUI({ shareOpen: true }));
  assert.ok(share.includes('data-share="native"'));
  assert.ok(share.includes('data-share="copy"'));
  const menu = renderDetail(REPORT, baseUI({ detailMenuOpen: true, itemState: { [REPORT.findings[0].path]: { pinned: true } } }));
  assert.ok(menu.includes('data-detailaction="unread"'));
  assert.ok(menu.includes("Unpin"), "reflects the pinned state");
});

/* ---------------------------------------------------------------- appbar + workspace */
test("renderAppbar exposes settings/info/theme and the principles popover", () => {
  assert.ok(renderAppbar(baseUI()).includes("data-settings-toggle"));
  const info = renderAppbar(baseUI({ infoOpen: true }));
  assert.ok(info.includes("How PromptDust works"));
  assert.ok(info.includes("Read-only"));
  assert.ok(info.includes("this Mac"), "device wording is OS-aware");
});

test("renderWorkspace stitches the ribbon + three panes", () => {
  const html = renderWorkspace(REPORT, INDEX, baseUI());
  assert.ok(html.includes("Exposure · high"));
  assert.ok(html.includes("data-inbox-toggle"));
  assert.ok(html.includes('data-group="Claude Code"'));
  assert.ok(html.includes("Reveal in Finder"));
});

test("renderToast is app-wide and shows on any screen (or nothing when empty)", () => {
  assert.equal(renderToast(baseUI({ toast: "" })), "");
  const toast = renderToast(baseUI({ toast: "Pinned" }));
  assert.ok(toast.includes('data-testid="toast"'));
  assert.ok(toast.includes("Pinned"));
  // rendered by renderApp even on the welcome screen (no workspace)
  const welcome = renderApp(baseUI({ screen: "welcome", toast: "Sharing on" }));
  assert.ok(welcome.includes('data-testid="toast"'));
  assert.ok(welcome.includes("Sharing on"));
});

/* ---------------------------------------------------------------- full-screen */
test("renderWelcome is OS-aware and offers the consent link, no delete", () => {
  const html = renderWelcome(baseUI({ os: "windows" }));
  assert.ok(html.includes("Scan this PC"));
  assert.ok(html.includes("Never leaves this PC"));
  assert.ok(html.includes("data-open-consent"));
  assert.ok(html.includes("no delete button"));
});

test("renderScanning / renderEmpty / renderPermission render their copy", () => {
  assert.ok(renderScanning(baseUI()).includes("Reading known locations"));
  assert.ok(renderEmpty(baseUI()).includes("No data from known AI tools found"));
  assert.ok(renderPermission(baseUI({ permMsg: "Boom" })).includes("Boom"));
  assert.ok(renderPermission(baseUI()).includes("Permission needed"));
});

/* ---------------------------------------------------------------- overlays */
test("renderConsent is off-by-default and equal-weight", () => {
  const html = renderConsent();
  assert.ok(html.includes("Optional · off by default"));
  assert.ok(html.includes('data-consent="no"'));
  assert.ok(html.includes('data-consent="yes"'));
  assert.ok(html.includes("DO_NOT_TRACK"));
});

test("renderSettings toggles telemetry and respects env suppression", () => {
  const on = renderSettings(baseUI({ telemetry: true }));
  assert.ok(on.includes("data-tele-toggle"));
  assert.ok(on.includes("data-tele-preview"));
  assert.ok(on.includes("data-diag-open"));
  assert.ok(on.includes("data-check-updates"), "check-for-updates row present");
  assert.ok(renderSettings(baseUI({ updateStatus: "You're on the latest version." })).includes("latest version"));
  const env = renderSettings(baseUI({ suppressedByEnv: true }));
  assert.ok(env.includes("Forced off by the environment"));
  assert.ok(!env.includes("data-tele-toggle"), "no live toggle when the env forces it off");
});

test("renderTelePreview / renderDiag show provided text or a loading state", () => {
  assert.ok(renderTelePreview(baseUI({ telePreviewText: '{"kind":"promptdust-telemetry"}' })).includes("promptdust-telemetry"));
  assert.ok(renderTelePreview(baseUI()).includes("Loading…"));
  assert.ok(renderDiag(baseUI({ diagText: "REDACTED-BUNDLE" })).includes("REDACTED-BUNDLE"));
  assert.ok(renderDiag(baseUI()).includes("data-diag-share"));
});

test("renderOverlays stacks only the open overlays", () => {
  assert.equal(renderOverlays(baseUI()), "");
  assert.ok(renderOverlays(baseUI({ consentOpen: true })).includes("Help improve PromptDust?"));
  const both = renderOverlays(baseUI({ settingsOpen: true, diagOpen: true }));
  assert.ok(both.includes("Feedback &amp; privacy"));
  assert.ok(both.includes("Diagnostics bundle"));
});

/* ---------------------------------------------------------------- top level */
test("renderApp routes by screen and falls back to empty without a report", () => {
  assert.ok(renderApp(baseUI({ screen: "welcome" })).includes("the dust your prompts leave behind"));
  assert.ok(renderApp(baseUI({ screen: "scanning" })).includes("Reading known locations"));
  assert.ok(renderApp(baseUI({ screen: "empty" })).includes("No data from known AI tools found"));
  assert.ok(renderApp(baseUI({ screen: "permission" })).includes("Permission needed"));
  assert.ok(renderApp(baseUI({ screen: "workspace" })).includes("Exposure · high"));
  assert.ok(renderApp(baseUI({ screen: "workspace", report: null })).includes("No data from known AI tools found"));
  assert.ok(renderApp(baseUI({ screen: "nonsense" })).includes("PromptDust"));
});

/* ---------------------------------------------------------------- safety */
test("all dynamic values are HTML-escaped (no injection from a hostile path/tool)", () => {
  const evil = {
    ...REPORT,
    summary: { total_findings: 1, total_bytes: 10, by_exposure: { high: 1 }, by_tool: { "<script>x</script>": 1 } },
    findings: [
      {
        tool: "<script>alert(1)</script>",
        path: "~/\"><img src=x onerror=alert(1)>/evil",
        exposure_level: "high",
        size_bytes: 10,
        file_count: 1,
        modified_epoch_secs: 1_760_000_000,
        amplifiers: ["cloud_sync"],
        why: "<b>bad</b>",
        guidance: ["<i>do</i>"],
      },
    ],
  };
  const ui = baseUI({ report: evil, selPath: evil.findings[0].path, expanded: new Set(["<script>alert(1)</script>"]) });
  const html = renderWorkspace(evil, INDEX, ui);
  assert.ok(!html.includes("<script>alert(1)</script>"), "tool name is escaped");
  assert.ok(!html.includes("<img src=x"), "path is escaped");
  assert.ok(!html.includes("<b>bad</b>"), "why is escaped");
  assert.ok(html.includes("&lt;script&gt;"));
});
