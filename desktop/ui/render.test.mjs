// Tests for the shared pure helpers (render.mjs). Run with `node --test`.

import test from "node:test";
import assert from "node:assert/strict";

import {
  EXPOSURE_ORDER,
  AMPLIFIERS,
  EXPOSURE_BAND_LEVEL,
  osTerms,
  detectOS,
  escapeHtml,
  humanSize,
  exposureRank,
  formatDate,
  hasFindings,
  reportToMarkdown,
} from "./render.mjs";

test("osTerms is OS-aware and degrades unknown to neutral wording", () => {
  assert.deepEqual(osTerms("macos"), { device: "this Mac", fileManager: "Finder", backup: "Time Machine backups" });
  assert.equal(osTerms("windows").device, "this PC");
  assert.equal(osTerms("linux").device, "this computer");
  assert.equal(osTerms("beos").device, "this computer");
  assert.equal(osTerms(undefined).fileManager, "the file manager");
});

test("detectOS reads the user-agent and falls back to unknown", () => {
  assert.equal(detectOS("Mozilla/5.0 (Windows NT 10.0)"), "windows");
  assert.equal(detectOS("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15)"), "macos");
  assert.equal(detectOS("Mozilla/5.0 (X11; Linux x86_64)"), "linux");
  assert.equal(detectOS("Node.js"), "unknown");
  assert.equal(detectOS(""), "unknown");
  assert.equal(detectOS(undefined), "unknown");
});

test("escapeHtml neutralizes every HTML metacharacter and nullish input", () => {
  assert.equal(escapeHtml(`<a href="x" title='y'>&</a>`), "&lt;a href=&quot;x&quot; title=&#39;y&#39;&gt;&amp;&lt;/a&gt;");
  assert.equal(escapeHtml(null), "");
  assert.equal(escapeHtml(undefined), "");
  assert.equal(escapeHtml(42), "42");
});

test("humanSize scales bytes through the units", () => {
  assert.equal(humanSize(0), "0 B");
  assert.equal(humanSize(512), "512 B");
  assert.equal(humanSize(1024), "1.0 KB");
  assert.equal(humanSize(1_048_576), "1.0 MB");
  assert.equal(humanSize(1_073_741_824), "1.0 GB");
  assert.equal(humanSize(1_099_511_627_776), "1.0 TB");
  assert.equal(humanSize("nan"), "0 B");
});

test("exposureRank orders by attention, unknown last", () => {
  assert.equal(exposureRank("critical"), 0);
  assert.equal(exposureRank("info"), 4);
  assert.equal(exposureRank("mystery"), EXPOSURE_ORDER.length);
});

test("formatDate renders yyyy-mm-dd, or null for absent/invalid input", () => {
  assert.equal(formatDate(1_760_000_000), "2025-10-09");
  assert.equal(formatDate(0), null);
  assert.equal(formatDate(null), null);
  assert.equal(formatDate(Infinity), null); // Date → Invalid → toISOString throws → caught
});

test("shared maps expose the expected vocabulary", () => {
  assert.deepEqual(EXPOSURE_ORDER, ["critical", "high", "medium", "low", "info"]);
  assert.equal(AMPLIFIERS.cloud_sync.kind, "offmachine");
  assert.equal(AMPLIFIERS.backup_swept.kind, "info");
  assert.equal(EXPOSURE_BAND_LEVEL.moderate, "medium");
  assert.equal(EXPOSURE_BAND_LEVEL.minimal, "info");
});

test("hasFindings reads the summary count or falls back to the array length", () => {
  assert.equal(hasFindings({ summary: { total_findings: 3 } }), true);
  assert.equal(hasFindings({ summary: { total_findings: 0 } }), false);
  assert.equal(hasFindings({ findings: [{}, {}] }), true);
  assert.equal(hasFindings({ findings: [] }), false);
  assert.equal(hasFindings({}), false);
});

const FULL = {
  host: { os: "macos" },
  disk_encryption: "on",
  mode: "inventory",
  exposure: { score: 71, band: "high" },
  assurance: { score: 88, band: "high" },
  coverage_gaps: [{}, {}],
  evasion_signals: [{}],
  interpretation: "Low exposure with a coverage gap — worth a look.",
  summary: { total_findings: 3, total_bytes: 213_056_716, by_tool: { "Claude Code": 2, Cursor: 1 } },
  findings: [
    { tool: "Cursor", path: "~/Cursor/state.vscdb", exposure_level: "high", size_bytes: 46_871_347, file_count: 1, amplifiers: ["world_readable"], why: "SQLite store.", guidance: ["Chmod 600."] },
    { tool: "Claude Code", path: "~/.claude/projects/api", exposure_level: "critical", size_bytes: 163_983_360, file_count: 214, evidence_class: "content", amplifiers: ["cloud_sync", "world_readable"], why: "Transcripts.", guidance: ["Move it.", "Keep it local."] },
    { tool: "Claude Code", path: "~/.claude/history.jsonl", exposure_level: "medium", size_bytes: 2_202_009, file_count: 1, amplifiers: [], why: "", guidance: [] },
  ],
  warnings: [{ path: "~/Library/Group Containers", reason: "permission denied" }, { reason: "probe timed out" }],
};

test("reportToMarkdown emits full, grouped, metadata-only markdown", () => {
  const md = reportToMarkdown(FULL);
  assert.ok(md.startsWith("# PromptDust report"));
  assert.ok(md.includes("Read-only inventory of AI data on this Mac"));
  assert.ok(md.includes("- Places: 3"));
  assert.ok(md.includes("- Tools: 2"));
  assert.ok(md.includes("- Total size: 203.2 MB"));
  assert.ok(md.includes("- Disk encryption: on"));
  assert.ok(md.includes("- Scan mode: inventory"));
  assert.ok(md.includes("- Exposure: 71/100 (high)"));
  assert.ok(md.includes("- Assurance: 88/100 (high) — how complete this look was"));
  assert.ok(md.includes("- Coverage gaps: 2"));
  assert.ok(md.includes("- Evasion signals: 1"));
  assert.ok(md.includes("- Low exposure with a coverage gap"));
  // Grouping: Claude Code holds the critical → sorts before Cursor.
  assert.ok(md.indexOf("## Claude Code (2)") < md.indexOf("## Cursor (1)"));
  // Facts: multi-file + evidence class.
  assert.ok(md.includes("**CRITICAL** `~/.claude/projects/api` — 156.4 MB · 214 files · evidence: content"));
  assert.ok(md.includes("  - Transcripts."));
  assert.ok(md.includes("  - Amplifiers: Cloud-synced, World-readable"));
  assert.ok(md.includes("  - Move it."));
  // Warnings with and without a path.
  assert.ok(md.includes("## Warnings (2)"));
  assert.ok(md.includes("- ~/Library/Group Containers: permission denied"));
  assert.ok(md.includes("- probe timed out"));
});

test("reportToMarkdown degrades cleanly when the optional fields are absent", () => {
  const md = reportToMarkdown({ summary: { total_findings: 0 }, findings: [] });
  assert.ok(md.includes("- Places: 0"));
  assert.ok(md.includes("- Disk encryption: unknown"));
  assert.ok(!md.includes("Scan mode"));
  assert.ok(!md.includes("Exposure:"));
  assert.ok(!md.includes("Assurance:"));
  assert.ok(!md.includes("Coverage gaps"));
  assert.ok(!md.includes("Warnings"));
});
