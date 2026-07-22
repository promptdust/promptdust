// Unit tests for the pure UI renderer. Run with `node --test` — no dependencies.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  escapeHtml,
  humanSize,
  exposureRank,
  meterFill,
  renderFindingRow,
  renderGroups,
  renderAttentionChips,
  offMachineNote,
  renderSummaryScreen,
  renderResults,
  renderDetail,
  hasFindings,
  reportToMarkdown,
  osTerms,
  detectOS,
  computeHeadline,
  renderRingOnePreview,
  renderDualScore,
} from "./render.mjs";

const FIXTURE = {
  host: { os: "macos" },
  disk_encryption: "on",
  warnings: [{ path: "/x/y", reason: "permission denied" }],
  findings: [
    {
      tool: "Claude Code",
      path: "/Users/x/.claude/projects/p",
      size_bytes: 3145728,
      file_count: 40,
      modified_epoch_secs: 1_760_000_000,
      inspection: {},
      amplifiers: ["cloud_sync", "world_readable"],
      exposure_level: "critical",
      why: "Verbatim transcripts of your Claude Code sessions.",
      guidance: ["Keep it out of synced folders."],
    },
    {
      tool: "Cursor",
      path: "/Users/x/Library/Application Support/Cursor/state.vscdb",
      size_bytes: 5000,
      file_count: 1,
      inspection: { row_count: 42 },
      amplifiers: ["backup_swept"],
      exposure_level: "medium",
      why: "Cursor SQLite chat store.",
      guidance: [],
    },
  ],
  summary: {
    total_findings: 2,
    total_bytes: 3150728,
    by_tool: { "Claude Code": 1, Cursor: 1 },
    by_exposure: { critical: 1, medium: 1 },
  },
};

test("humanSize scales units", () => {
  assert.equal(humanSize(512), "512 B");
  assert.equal(humanSize(1536), "1.5 KB");
  assert.equal(humanSize(3 * 1024 * 1024), "3.0 MB");
});

test("exposureRank orders critical before info", () => {
  assert.ok(exposureRank("critical") < exposureRank("medium"));
  assert.ok(exposureRank("medium") < exposureRank("info"));
  assert.equal(exposureRank("bogus"), 5);
});

test("meterFill: critical fills all 5 bars, info fills 1", () => {
  assert.equal(meterFill("critical"), 5);
  assert.equal(meterFill("high"), 4);
  assert.equal(meterFill("medium"), 3);
  assert.equal(meterFill("info"), 1);
  assert.equal(meterFill("bogus"), 1);
});

test("finding row: meter, level tag, path, size, amps, and stable data-idx", () => {
  const html = renderFindingRow(FIXTURE.findings[0], 7);
  assert.ok(html.includes('data-idx="7"'));
  assert.ok(html.includes('class="meter critical"'));
  assert.ok(html.includes('class="tag critical"'));
  assert.ok(html.includes("/Users/x/.claude/projects/p"));
  assert.ok(html.includes("3.0 MB"));
  assert.ok(html.includes("Cloud-synced")); // amplifier label
  // 5 meter bars, 5 filled for critical
  assert.equal((html.match(/class="bar on"/g) || []).length, 5);
});

test("groups: ordered by highest-attention tool, with per-tool counts", () => {
  const html = renderGroups(FIXTURE);
  // Claude Code (critical) must group before Cursor (medium)
  assert.ok(html.indexOf("Claude Code") < html.indexOf("Cursor"));
  assert.ok(html.includes('class="count">1<'));
});

test("groups: filtering keeps original index and drops empty tools", () => {
  const html = renderGroups(FIXTURE, "critical");
  assert.ok(html.includes("Claude Code"));
  assert.ok(!html.includes("Cursor")); // medium finding filtered out
  // original index of the critical finding is 0
  assert.ok(html.includes('data-idx="0"'));
});

test("attention chips are filter controls with counts + active state", () => {
  const html = renderAttentionChips(FIXTURE, "critical");
  assert.ok(html.includes('class="chip all"')); // All not active
  assert.ok(html.includes('class="chip critical active"')); // active level
  assert.ok(html.includes('data-level="critical"'));
  assert.ok(html.includes('data-level="medium"'));
});

test("off-machine note fires for cloud/git findings only", () => {
  assert.ok(offMachineNote(FIXTURE).includes("left this Mac"));
  assert.ok(offMachineNote(FIXTURE).includes("cloud-synced"));
  const local = { findings: [{ ...FIXTURE.findings[1] }] };
  assert.equal(offMachineNote(local), "");
});

test("summary screen: headline lead, stats, chips, disk line — and never a verdict", () => {
  const html = renderSummaryScreen(FIXTURE);
  // Leads with the computed headline (Claude Code is the largest store), not a generic title.
  assert.ok(html.includes("3.0 MB of Claude Code data"));
  assert.ok(!html.includes("Your AI footprint")); // the generic h1 is gone when a fact leads
  assert.ok(html.includes(">2<") && html.includes("place"));
  assert.ok(html.includes("Disk encryption · on"));
  assert.ok(!/you are safe|you're secure|all clear|you're clean/i.test(html));
});

test("computeHeadline picks the most striking fact per its priority chain", () => {
  const mk = (findings, extra = {}) => ({
    host: { os: "macos" },
    findings,
    summary: { total_findings: findings.length, total_bytes: findings.reduce((s, f) => s + f.size_bytes, 0) },
    ...extra,
  });

  // 1 · A tool's footprint ≥ 1 MB dominates → "X of <tool> data".
  const big = computeHeadline(
    mk([
      { tool: "Claude Code", size_bytes: 754 * 1024 * 1024, amplifiers: [] },
      { tool: "Claude Code", size_bytes: 10 * 1024 * 1024, amplifiers: [] },
      { tool: "Cursor", size_bytes: 2000, amplifiers: [] },
    ]),
  );
  assert.equal(big.lead, "764.0 MB of Claude Code data");
  assert.equal(big.sub, "across 2 places"); // 2 Claude Code stores

  // Single dominant store → "in one place" (singular).
  assert.equal(
    computeHeadline(mk([{ tool: "Ollama", size_bytes: 5 * 1024 * 1024, amplifiers: [] }])).sub,
    "in one place",
  );

  // 2 · Nothing large on disk, but stores have left the machine → off-machine lead (OS-aware).
  const off = computeHeadline(
    mk([
      { tool: "Cursor", size_bytes: 4000, amplifiers: ["cloud_sync"] },
      { tool: "Aider", size_bytes: 3000, amplifiers: ["in_git_repo"] },
    ]),
  );
  assert.equal(off.lead, "2 AI stores have left this Mac");
  assert.ok(off.sub.includes("2 places") && off.sub.includes("2 tools"));
  // A single off-machine store uses singular "has left".
  assert.ok(
    computeHeadline(mk([{ tool: "Cursor", size_bytes: 900, amplifiers: ["cloud_sync"] }])).lead.startsWith(
      "1 AI store has left",
    ),
  );

  // 3 · Small + local, spread across ≥2 tools → breadth lead.
  const wide = computeHeadline(
    mk([
      { tool: "Cursor", size_bytes: 900, amplifiers: [] },
      { tool: "Aider", size_bytes: 800, amplifiers: [] },
      { tool: "Continue", size_bytes: 700, amplifiers: [] },
    ]),
  );
  assert.equal(wide.lead, "AI data from 3 tools");
  assert.ok(wide.sub.startsWith("in 3 places"));

  // One tool across several small stores is NOT "breadth" (that needs ≥2 tools) → neutral lead.
  const oneToolManyPlaces = computeHeadline(
    mk([
      { tool: "Cursor", size_bytes: 900, amplifiers: [] },
      { tool: "Cursor", size_bytes: 800, amplifiers: [] },
      { tool: "Cursor", size_bytes: 700, amplifiers: [] },
    ]),
  );
  assert.equal(oneToolManyPlaces.lead, "Your AI footprint");
  assert.ok(oneToolManyPlaces.sub.startsWith("3 places"));

  // 4 · Sparse (one small local store) → a calm, neutral lead.
  const sparse = computeHeadline(mk([{ tool: "Ollama", size_bytes: 400, amplifiers: [] }]));
  assert.equal(sparse.lead, "Your AI footprint");
  assert.ok(sparse.sub.startsWith("1 place"));

  // No fixture yields verdict/alarm language.
  for (const h of [big, off, wide, oneToolManyPlaces, sparse]) {
    assert.ok(!/safe|secure|clean|at risk|exposed|danger|vulnerab/i.test(`${h.lead} ${h.sub}`));
  }
});

test("computeHeadline degrades safely on missing or partial fields", () => {
  // No findings / no summary → a neutral headline string, never a throw.
  const empty = computeHeadline({});
  assert.equal(typeof empty.lead, "string");
  assert.equal(typeof empty.sub, "string");
  assert.ok(empty.lead.length > 0);
  // Findings missing size_bytes / amplifiers / tool don't throw and leak no undefined/NaN.
  const partial = computeHeadline({ findings: [{ tool: "Cursor" }, {}], summary: {} });
  assert.equal(typeof partial.lead, "string");
  assert.ok(!/undefined|NaN/.test(`${partial.lead} ${partial.sub}`));
  // A dominant store whose finding lacks `tool` renders "AI", never the literal "undefined".
  const noTool = computeHeadline({ findings: [{ size_bytes: 2 * 1024 * 1024 }] });
  assert.equal(noTool.lead, "2.0 MB of AI data");
  // A non-integer total_findings falls back to the finding count — never "NaN places".
  const nanCount = computeHeadline({
    findings: [{ tool: "Cursor", size_bytes: 500, amplifiers: [] }],
    summary: { total_findings: NaN },
  });
  assert.ok(!/NaN/.test(`${nanCount.lead} ${nanCount.sub}`));
  assert.ok(nanCount.sub.startsWith("1 place"));
});

test("computeHeadline is deterministic — same scan, same headline; ties resolve stably", () => {
  const data = {
    host: { os: "macos" },
    findings: [
      { tool: "Bravo", size_bytes: 2 * 1024 * 1024, amplifiers: [] },
      { tool: "Alpha", size_bytes: 2 * 1024 * 1024, amplifiers: [] },
    ],
    summary: { total_findings: 2, total_bytes: 4 * 1024 * 1024 },
  };
  assert.deepEqual(computeHeadline(data), computeHeadline(data)); // pure → identical output
  // Equal byte totals: the first tool seen wins (strict >, so no run-to-run flip).
  assert.equal(computeHeadline(data).lead, "2.0 MB of Bravo data");
});

test("the headline escapes the tool name it interpolates (no injection via summary)", () => {
  const html = renderSummaryScreen({
    host: { os: "macos" },
    findings: [{ tool: "X<script>", size_bytes: 2 * 1024 * 1024, amplifiers: [] }],
    summary: { total_findings: 1, total_bytes: 2 * 1024 * 1024, by_tool: { "X<script>": 1 } },
  });
  assert.ok(!html.includes("<script>"));
  assert.ok(html.includes("&lt;script&gt;"));
});

test("summary screen: surfaces the dual score as numbers, never [object Object]", () => {
  const scored = {
    ...FIXTURE,
    mode: "inventory",
    exposure: { score: 62, band: "high" },
    assurance: { score: 90, band: "high", corroboration_bonus: 0 },
    interpretation: "Confirmed exposure — act now.",
  };
  const html = renderSummaryScreen(scored);
  assert.ok(html.includes("62/100") && html.includes("90/100"));
  assert.ok(html.includes("Confirmed exposure"));
  assert.ok(!html.includes("[object Object]"), "the assurance object must render as numbers");
  assert.ok(!/credibly clean|is clean|all clear/i.test(html)); // never a verdict
});

test("markdown export: dual score renders as numbers, not [object Object]", () => {
  const scored = {
    ...FIXTURE,
    exposure: { score: 62, band: "high" },
    assurance: { score: 90, band: "high", corroboration_bonus: 0 },
    interpretation: "Confirmed exposure — act now.",
    coverage_gaps: [
      { id: "a", note: "GAPNOTE_ALPHA", penalty: 5 },
      { id: "b", note: "GAPNOTE_BETA", penalty: 5 },
    ],
    evasion_signals: [{ id: "c", note: "EVASIONNOTE_GAMMA", penalty: 8 }],
  };
  const md = reportToMarkdown(scored);
  assert.ok(md.includes("- Exposure: 62/100 (high)"));
  assert.ok(md.includes("- Assurance: 90/100 (high)"));
  assert.ok(md.includes("- Coverage gaps: 2") && md.includes("- Evasion signals: 1")); // counts, not notes
  assert.ok(!/GAPNOTE_|EVASIONNOTE_/.test(md)); // the gap/evasion *notes* are never emitted
  assert.ok(!md.includes("[object Object]"));
  // Absent gaps/evasions omit the lines entirely.
  const noGaps = reportToMarkdown({ ...FIXTURE, exposure: { score: 1, band: "minimal" }, assurance: { score: 99, band: "high" } });
  assert.ok(!noGaps.includes("Coverage gaps") && !noGaps.includes("Evasion signals"));
});

test("renderDualScore: exposure + assurance-as-coverage + interpretation, never a grade", () => {
  const html = renderDualScore({
    exposure: { score: 62, band: "high" },
    assurance: { score: 55, band: "partial", corroboration_bonus: 0 },
    interpretation: "Likely exposure, obscured — investigate.",
    coverage_gaps: [
      { id: "a", note: "GAPNOTE_ALPHA", penalty: 10 },
      { id: "b", note: "GAPNOTE_BETA", penalty: 5 },
    ],
    evasion_signals: [{ id: "c", note: "EVASIONNOTE_GAMMA", penalty: 8 }],
  });
  assert.ok(html.includes("62/100") && html.includes("Exposure · high"));
  assert.ok(html.includes("55/100") && html.includes("Assurance · partial"));
  assert.ok(html.includes("how complete this look was")); // instrumentation framing
  assert.ok(html.includes("2 coverage gaps") && html.includes("1 evasion signal")); // counts, plural/singular
  assert.ok(!/GAPNOTE_|EVASIONNOTE_/.test(html)); // metadata-only: the gap/evasion notes never render
  assert.ok(html.includes("Likely exposure, obscured")); // the interpretation line
  // Exposure carries a palette level class; Assurance is NOT on the exposure gradient.
  assert.ok(html.includes('class="score exposure high"'));
  assert.ok(html.includes('class="score assurance"'));
  // No verdict / grade / reassurance language.
  assert.ok(!/\b(safe|clean|secure|passed?|failed?|grade)\b/i.test(html));
});

test("renderDualScore maps endpoint bands onto the gradient palette (unknown → the calmest)", () => {
  const mk = (band) =>
    renderDualScore({ exposure: { score: 10, band }, assurance: { score: 80, band: "high" } });
  assert.ok(mk("minimal").includes('class="score exposure info"')); // minimal → info (coolest)
  assert.ok(mk("low").includes('class="score exposure low"'));
  assert.ok(mk("moderate").includes('class="score exposure medium"')); // moderate → medium
  assert.ok(mk("high").includes('class="score exposure high"'));
  assert.ok(mk("critical").includes('class="score exposure critical"'));
  // An out-of-set band clamps to a known palette level ("info") — never an unstyled/off-palette
  // class. (The gradient itself is cool→warm-never-green in CSS; the class is always one of these five.)
  assert.ok(mk("bogus").includes('class="score exposure info"'));
});

test("renderDualScore degrades to empty when either score is absent", () => {
  assert.equal(renderDualScore({}), "");
  assert.equal(renderDualScore({ exposure: { score: 1, band: "low" } }), ""); // needs both
  assert.equal(renderDualScore({ assurance: { score: 1, band: "low" } }), "");
});

test("renderDualScore omits the coverage note when there are no gaps or evasions", () => {
  const html = renderDualScore({
    exposure: { score: 5, band: "minimal" },
    assurance: { score: 95, band: "high" },
  });
  assert.ok(html.includes("how complete this look was"));
  assert.ok(!/coverage gap|evasion signal/.test(html));
});

test("results screen: header copy, filter line when filtered, groups", () => {
  const plain = renderResults(FIXTURE, null);
  assert.ok(plain.includes("What's on this Mac"));
  assert.ok(plain.includes("back-summary")); // can navigate back to the summary
  assert.ok(!plain.includes("show all")); // no filter line when unfiltered
  const filtered = renderResults(FIXTURE, "critical");
  assert.ok(filtered.includes("show all"));
  assert.ok(filtered.includes("Showing"));
  // The dual score renders at the head of the results screen too (guards the wiring point).
  const scored = renderResults(
    { ...FIXTURE, exposure: { score: 62, band: "high" }, assurance: { score: 90, band: "high" } },
    null,
  );
  assert.ok(scored.includes("Exposure · high") && scored.includes("Assurance · high"));
  assert.ok(!renderResults(FIXTURE, null).includes("Exposure ·")); // absent scores → no dual score
});

test("detail: amplifiers, guidance, reveal path, and NO delete affordance", () => {
  const html = renderDetail(FIXTURE.findings[0], FIXTURE.host.os);
  assert.ok(html.includes("Claude Code"));
  assert.ok(html.includes("Amplifiers"));
  assert.ok(html.includes("On your other machines")); // cloud_sync note
  assert.ok(html.includes("Reveal in Finder")); // macOS file manager
  assert.ok(html.includes("What you can do"));
  assert.ok(html.includes("Keep it out of synced folders."));
  assert.ok(html.includes('data-path="/Users/x/.claude/projects/p"'));
  assert.ok(!/delete|erase|quarantine/i.test(html)); // reveal is the only action
});

test("hasFindings reflects the summary", () => {
  assert.equal(hasFindings(FIXTURE), true);
  assert.equal(hasFindings({ summary: { total_findings: 0 }, findings: [] }), false);
});

test("reportToMarkdown: grouped, metadata-only, highest-attention tool first", () => {
  const md = reportToMarkdown(FIXTURE);
  assert.ok(md.startsWith("# PromptDust report"));
  assert.ok(md.includes("- Disk encryption: on"));
  assert.ok(md.includes("## Claude Code (1)"));
  assert.ok(md.includes("**CRITICAL**"));
  assert.ok(md.includes("Amplifiers: Cloud-synced"));
  // Claude Code (critical) is listed before Cursor (medium)
  assert.ok(md.indexOf("Claude Code") < md.indexOf("Cursor"));
});

test("all dynamic fields are HTML-escaped (no injection)", () => {
  const evil = {
    disk_encryption: "on",
    findings: [
      {
        tool: "X<script>",
        path: "/a/<b>.jsonl",
        size_bytes: 1,
        file_count: 1,
        amplifiers: [],
        exposure_level: "high",
        why: "<img src=x onerror=alert(1)>",
        guidance: ["<b>do</b>"],
      },
    ],
    summary: { total_findings: 1, total_bytes: 1, by_tool: { X: 1 }, by_exposure: { high: 1 } },
  };
  const html = renderResults(evil, null) + renderDetail(evil.findings[0]);
  assert.ok(!html.includes("<img src=x"));
  assert.ok(!html.includes("<script>"));
  assert.ok(html.includes("&lt;img src=x"));
});

test("new fields surface additively (mode, assurance, evidence_class) and are ignore-safe", () => {
  const enriched = {
    ...FIXTURE,
    mode: "inventory",
    exposure: { score: 62, band: "high" },
    assurance: { score: 90, band: "high", corroboration_bonus: 0 },
    interpretation: "Confirmed exposure — act now.",
    findings: FIXTURE.findings.map((f) => ({ ...f, evidence_class: "presence" })),
  };
  const summary = renderSummaryScreen(enriched);
  assert.ok(summary.includes("Mode · inventory"));
  assert.ok(summary.includes("62/100") && summary.includes("90/100"));
  assert.ok(renderDetail(enriched.findings[0]).includes("evidence: presence"));
  const md = reportToMarkdown(enriched);
  assert.ok(md.includes("- Scan mode: inventory"));
  assert.ok(md.includes("- Assurance: 90/100 (high)"));
  assert.ok(md.includes("evidence: presence"));
  // Ignore-safe: the original fixture (no new fields) renders without them.
  assert.ok(!renderSummaryScreen(FIXTURE).includes("Mode ·"));
  assert.ok(!renderDetail(FIXTURE.findings[0]).includes("evidence:"));
});

test("detectOS reads the OS from a user-agent and degrades unknown agents to neutral", () => {
  assert.equal(detectOS("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605"), "macos");
  assert.equal(detectOS("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537"), "windows");
  assert.equal(detectOS("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537"), "linux");
  assert.equal(detectOS(""), "unknown");
  assert.equal(detectOS(undefined), "unknown");
  // Round-trips into osTerms so the welcome copy is right — and never a wrong OS name.
  assert.equal(osTerms(detectOS("Mozilla/5.0 (Windows NT 10.0)")).device, "this PC");
  assert.equal(osTerms(detectOS("some unknown agent")).device, "this computer");
});

test("osTerms maps each OS and degrades the unknown case to neutral wording", () => {
  assert.deepEqual(osTerms("macos"), {
    device: "this Mac",
    fileManager: "Finder",
    backup: "Time Machine backups",
  });
  assert.deepEqual(osTerms("windows"), {
    device: "this PC",
    fileManager: "File Explorer",
    backup: "File History",
  });
  assert.equal(osTerms("linux").device, "this computer");
  // Anything unrecognized (or absent) → neutral wording, never a wrong OS name.
  const neutral = osTerms(undefined);
  assert.equal(neutral.device, "this computer");
  assert.equal(neutral.fileManager, "the file manager");
  assert.ok(!/Mac|Finder|Time Machine|PC|Explorer/.test(JSON.stringify(osTerms("bogus"))));
});

test("copy follows host.os across results, off-machine note, detail, and export", () => {
  // Windows — device, file manager, and amplifier note all de-Mac'd.
  const win = { ...FIXTURE, host: { os: "windows" } };
  assert.ok(renderResults(win, null).includes("What's on this PC"));
  assert.ok(offMachineNote(win).includes("left this PC"));
  const winDetail = renderDetail(win.findings[0], "windows");
  assert.ok(winDetail.includes("Reveal in File Explorer"));
  assert.ok(winDetail.includes("Other accounts on this PC"));
  assert.ok(!/this Mac|Finder|Time Machine/.test(winDetail));
  assert.ok(reportToMarkdown(win).includes("AI data on this PC"));

  // Linux — neutral device, backup phrasing follows suit, no Mac/PC leakage.
  const lin = { ...FIXTURE, host: { os: "linux" } };
  assert.ok(renderResults(lin, null).includes("What's on this computer"));
  assert.ok(!/this Mac|this PC/.test(renderResults(lin, null)));
  assert.ok(renderDetail(lin.findings[1], "linux").includes("Kept in system backups"));

  // macOS (the fixture default) — the original Mac wording is preserved.
  assert.ok(renderDetail(FIXTURE.findings[1], "macos").includes("Kept in Time Machine backups"));

  // Absent host.os → neutral, and the rendered output leaks no Mac/PC/Finder wording.
  const unknown = { ...FIXTURE, host: {} };
  const unknownRender =
    renderResults(unknown, null) +
    offMachineNote(unknown) +
    renderDetail(unknown.findings[0], undefined);
  assert.ok(unknownRender.includes("What's on this computer"));
  assert.ok(unknownRender.includes("left this computer"));
  assert.ok(!/this Mac|this PC|Finder|File Explorer|Time Machine|File History/.test(unknownRender));
});

test("ring-1 preview: honest 'what it would read' list, gated, no collected data", () => {
  const html = renderRingOnePreview();
  // The honest preview of the four read surfaces the deeper ring would cover.
  assert.ok(/what this would read/i.test(html));
  assert.ok(html.includes("Running AI processes"));
  assert.ok(html.includes("Listening ports"));
  assert.ok(/persistence/i.test(html));
  assert.ok(/recently-used ai tools/i.test(html)); // matches the "& recently used" label
  assert.ok(/repository markers/i.test(html));
  // Read-only / on-device framing — never a claim it reads content or reaches the network.
  assert.ok(/read-only/i.test(html));
  // Gated: it states plainly that nothing is collected yet — no Ring-1 data is shown.
  assert.ok(/not collected yet|coming soon/i.test(html));
  // No verdict language (FR-5 by discipline — the UI copy is not machine-linted).
  assert.ok(!/\b(safe|secure|clean)\b/i.test(html));
});

test("ring-1 preview: OS-aware device term, escaped", () => {
  assert.ok(renderRingOnePreview("this PC").includes("on this PC"));
  // Defaults to the neutral device term.
  assert.ok(renderRingOnePreview().includes("this computer"));
  // The device term is HTML-escaped.
  assert.ok(renderRingOnePreview("<b>x</b>").includes("&lt;b&gt;x&lt;/b&gt;"));
});
