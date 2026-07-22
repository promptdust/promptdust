// Pure rendering for the PromptDust desktop UI. No DOM, no Tauri imports — every
// function maps report data to an HTML string, so it is unit-testable with
// `node --test`. All dynamic values are HTML-escaped (the data is metadata-only, but
// we never trust-render).

export const EXPOSURE_ORDER = ["critical", "high", "medium", "low", "info"];

// Amplifier vocabulary: a plain-English note plus its "kind" —
//  offmachine = defines Critical (data left the machine); local = raises the level by
//  one; info = shown but never raises the level (backup_swept / large_growth).
export const AMPLIFIERS = {
  cloud_sync: { label: "Cloud-synced", note: "On your other machines and a provider's servers.", kind: "offmachine" },
  in_git_repo: { label: "In a git repo", note: "Could be committed and pushed.", kind: "offmachine" },
  world_readable: { label: "World-readable", note: "Other accounts on {device} can read it.", kind: "local" },
  unencrypted_disk: { label: "Unencrypted disk", note: "A stolen disk would expose it.", kind: "local" },
  backup_swept: { label: "In backups", note: "Kept in {backup}.", kind: "info" },
  large_growth: { label: "Large", note: "Unusually large.", kind: "info" },
};

// OS-aware vocabulary — the device, its file manager, and its backup system. `os` is the
// report's `host.os` (`std::env::consts::OS`: "macos"/"windows"/"linux"); anything else
// (or absent) degrades to neutral, never-wrong wording. Used to de-Mac the copy so a
// Windows/Linux user isn't told about "this Mac" / "Finder" / "Time Machine".
export function osTerms(os) {
  switch (os) {
    case "macos":
      return { device: "this Mac", fileManager: "Finder", backup: "Time Machine backups" };
    case "windows":
      return { device: "this PC", fileManager: "File Explorer", backup: "File History" };
    case "linux": // no distinctive vocabulary — the neutral fallback fits
    default:
      return { device: "this computer", fileManager: "the file manager", backup: "system backups" };
  }
}

// Resolve an OS key from a browser user-agent — used pre-scan (before a report's `host.os`
// exists) to pick the welcome-screen wording. Emits the same keys `osTerms` consumes;
// anything unrecognized → "unknown" (→ neutral copy, never a wrong OS name).
export function detectOS(userAgent) {
  const ua = userAgent || "";
  if (/Windows/i.test(ua)) return "windows";
  if (/Mac OS X|Macintosh/i.test(ua)) return "macos";
  if (/Linux|X11/i.test(ua)) return "linux";
  return "unknown";
}

// The Ring-1 consent affordance preview (ADR-015). The deeper "usage" ring — what's running
// and recently used — is a SEPARATE explicit opt-in. Its collectors are not built yet, so
// this renders only an honest preview of *what it would read* (all read-only, on-device) and
// states plainly that nothing is collected yet. It never shows any Ring-1 data. The list
// mirrors the affordance label ("what's running & recently used") so the two agree.
export function renderRingOnePreview(device = "this computer") {
  const reads = [
    "Running AI processes",
    "Listening ports",
    "Persistence (launch agents, services, autostart)",
    "Recently-used AI tools (last-run times)",
    "Repository markers (.git and similar)",
  ];
  const items = reads.map((r) => `<li>${escapeHtml(r)}</li>`).join("");
  return `<div class="ring-preview">
    <p class="ring-preview-lead">What this would read — all read-only, on ${escapeHtml(device)}:</p>
    <ul class="ring-preview-list">${items}</ul>
    <p class="ring-preview-note muted">Not collected yet — this deeper check is coming soon.</p>
  </div>`;
}

export function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

export function humanSize(bytes) {
  const b = Number(bytes) || 0;
  if (b < 1024) return `${b} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = b / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v.toFixed(1)} ${units[i]}`;
}

// "" for one, "s" for zero or many — the plural suffix for count labels across the UI copy.
function plural(n) {
  return n === 1 ? "" : "s";
}

export function exposureRank(level) {
  const idx = EXPOSURE_ORDER.indexOf(level);
  return idx === -1 ? EXPOSURE_ORDER.length : idx;
}

// Bars filled on the 5-segment meter: critical=5 … info=1 (unknown → 1).
export function meterFill(level) {
  const idx = EXPOSURE_ORDER.indexOf(level);
  return idx === -1 ? 1 : EXPOSURE_ORDER.length - idx;
}

function ampKind(a) {
  return AMPLIFIERS[a]?.kind ?? "local";
}
function ampLabel(a) {
  return AMPLIFIERS[a]?.label ?? String(a).replaceAll("_", " ");
}
// The amplifier note, with any `{device}`/`{backup}`/`{fileManager}` placeholder filled from
// `terms` (OS-aware wording). Notes without a placeholder pass through unchanged; an unknown
// key is left literal (so a typo surfaces visibly rather than vanishing to an empty string).
function ampNote(a, terms) {
  return (AMPLIFIERS[a]?.note ?? "").replace(/\{(\w+)\}/g, (match, key) =>
    Object.hasOwn(terms, key) ? terms[key] : match,
  );
}

function levelMeter(level) {
  let bars = "";
  const fill = meterFill(level);
  for (let i = 1; i <= EXPOSURE_ORDER.length; i += 1) {
    bars += `<span class="bar${i <= fill ? " on" : ""}"></span>`;
  }
  return `<span class="meter ${escapeHtml(level)}" aria-hidden="true">${bars}</span>`;
}

function levelTag(level) {
  return `<span class="tag ${escapeHtml(level)}">${escapeHtml(level)}</span>`;
}

function ampTag(a) {
  return `<span class="amp ${ampKind(a)}">${escapeHtml(ampLabel(a))}</span>`;
}

function formatDate(epochSecs) {
  if (!epochSecs) return null;
  try {
    return new Date(epochSecs * 1000).toISOString().slice(0, 10);
  } catch {
    return null;
  }
}

// One finding row (a button → opens the detail panel). `idx` is the finding's index
// in the *original* data.findings, preserved across filtering for detail lookup.
export function renderFindingRow(f, idx) {
  const amps = (f.amplifiers ?? []).map(ampTag).join("");
  return `<button class="finding" data-idx="${idx}" data-level="${escapeHtml(f.exposure_level)}" type="button">
    <span class="finding-top">
      ${levelMeter(f.exposure_level)}
      ${levelTag(f.exposure_level)}
      <span class="path">${escapeHtml(f.path)}</span>
      <span class="size num">${escapeHtml(humanSize(f.size_bytes))}</span>
      <span class="chev" aria-hidden="true">›</span>
    </span>
    ${amps ? `<span class="finding-sub"><span class="amps">${amps}</span></span>` : ""}
  </button>`;
}

function minRank(list) {
  return list.reduce((m, { f }) => Math.min(m, exposureRank(f.exposure_level)), Infinity);
}

// Group findings by tool (optionally filtered to one level), tools ordered so the ones
// holding the highest-attention items come first. Empty groups are dropped.
export function renderGroups(data, filter = null) {
  const items = (data.findings ?? []).map((f, idx) => ({ f, idx }));
  const shown = filter ? items.filter((it) => it.f.exposure_level === filter) : items;

  const byTool = new Map();
  for (const it of shown) {
    if (!byTool.has(it.f.tool)) byTool.set(it.f.tool, []);
    byTool.get(it.f.tool).push(it);
  }
  const groups = [...byTool.entries()];
  for (const [, list] of groups) {
    list.sort((a, b) => exposureRank(a.f.exposure_level) - exposureRank(b.f.exposure_level));
  }
  groups.sort((a, b) => minRank(a[1]) - minRank(b[1]));

  if (!groups.length) return `<p class="muted empty-filter">Nothing at this level.</p>`;

  return groups
    .map(
      ([tool, list]) => `<section class="tool">
      <h2>${escapeHtml(tool)} <span class="count">${list.length}</span></h2>
      <div class="findings">${list.map(({ f, idx }) => renderFindingRow(f, idx)).join("")}</div>
    </section>`,
    )
    .join("");
}

// The Attention chips — both a stat and the filter control. `filter` marks the active
// level (null = "All"). Empty levels are omitted.
export function renderAttentionChips(data, filter = null) {
  const by = data.summary?.by_exposure ?? {};
  const total = data.summary?.total_findings ?? 0;
  const chips = [
    `<button class="chip all${!filter ? " active" : ""}" data-level="" type="button">All <span class="n num">${total}</span></button>`,
  ];
  for (const lvl of EXPOSURE_ORDER) {
    if (!by[lvl]) continue;
    chips.push(
      `<button class="chip ${lvl}${filter === lvl ? " active" : ""}" data-level="${lvl}" type="button">${lvl} <span class="n num">${by[lvl]}</span></button>`,
    );
  }
  return `<div class="attention" role="group" aria-label="Filter by attention level">${chips.join("")}</div>`;
}

// Findings that have left (or could leave) the machine — cloud-synced or in a git working
// tree. The single definition of "off-machine", shared by the note and the headline.
function offMachineFindings(data) {
  return (data.findings ?? []).filter((f) =>
    (f.amplifiers ?? []).some((a) => ampKind(a) === "offmachine"),
  );
}

// The dynamic "these left this device" line — computed from off-machine amplifiers.
export function offMachineNote(data) {
  const off = offMachineFindings(data);
  if (!off.length) return "";
  const terms = osTerms(data.host?.os);
  const cloud = off.filter((f) => (f.amplifiers ?? []).includes("cloud_sync")).length;
  const git = off.filter((f) => (f.amplifiers ?? []).includes("in_git_repo")).length;
  const parts = [];
  if (cloud) parts.push(`${cloud} cloud-synced`);
  if (git) parts.push(`${git} in git`);
  return `<div class="note offmachine"><strong>${off.length} left ${escapeHtml(terms.device)}</strong> · ${escapeHtml(parts.join(" · "))}</div>`;
}

function toolCount(data) {
  return Object.keys(data.summary?.by_tool ?? {}).length;
}

// A tool's total footprint must clear this to headline by size; below it, size isn't striking.
const DOMINANT_TOOL_BYTES = 1024 * 1024; // 1 MB

// The headline revelation — the single most striking fact from the scan, phrased calmly and
// specifically so the first scan lands. Pure and metadata-only: reads
// counts / sizes / tool names, never content, and states a fact — never a verdict. Returns
// `{ lead, sub }` for the top of the summary. Deterministic priority chain, so the same scan
// always yields the same headline:
//   1 · a dominant store's sheer size (the visceral "that much?" whoa) — this deliberately
//       leads with magnitude, complementing (not replacing) the exposure model the Attention
//       chips + off-machine note surface just below; retune here if that emphasis changes.
//   2 · data that has left the machine · 3 · breadth across tools · 4 · a neutral fallback.
export function computeHeadline(data) {
  const findings = data.findings ?? [];
  const totalFindings = data.summary?.total_findings;
  const places = Number.isFinite(totalFindings) ? totalFindings : findings.length;
  const totalBytes = data.summary?.total_bytes ?? 0;

  // Footprint by tool (bytes + store count) — names the tool you've accrued the most under.
  const byTool = new Map();
  for (const f of findings) {
    const agg = byTool.get(f.tool) ?? { bytes: 0, count: 0 };
    agg.bytes += Number(f.size_bytes) || 0;
    agg.count += 1;
    byTool.set(f.tool, agg);
  }
  let topTool = null;
  for (const [tool, agg] of byTool) {
    if (!topTool || agg.bytes > topTool.bytes) topTool = { tool, ...agg };
  }
  const tools = byTool.size;

  // 1 · A tool's footprint dominates — the most visceral "that much?" fact. (The tool name is
  // a required definition field; `|| "AI"` is a belt-and-braces guard so a malformed finding
  // never renders "undefined".)
  if (topTool && topTool.bytes >= DOMINANT_TOOL_BYTES) {
    return {
      lead: `${humanSize(topTool.bytes)} of ${topTool.tool || "AI"} data`,
      sub: topTool.count === 1 ? "in one place" : `across ${topTool.count} places`,
    };
  }
  // 2 · Little on disk, but some has left the machine — the most consequential fact.
  const offMachine = offMachineFindings(data);
  if (offMachine.length) {
    const terms = osTerms(data.host?.os);
    return {
      lead: `${offMachine.length} AI store${plural(offMachine.length)} ${offMachine.length === 1 ? "has" : "have"} left ${terms.device}`,
      sub: `${places} place${plural(places)} · ${tools} tool${plural(tools)}`,
    };
  }
  // 3 · Breadth — data spread across several tools.
  if (tools >= 2) {
    return {
      lead: `AI data from ${tools} tools`,
      sub: `in ${places} place${plural(places)} · ${humanSize(totalBytes)}`,
    };
  }
  // 4 · Sparse — a calm, neutral lead.
  return {
    lead: "Your AI footprint",
    sub: `${places} place${plural(places)} · ${humanSize(totalBytes)}`,
  };
}

// Endpoint Exposure bands (minimal…critical) mapped onto the UI's cool→warm level palette
// (info…critical) — the score reuses the same never-green gradient as the finding meters.
const EXPOSURE_BAND_LEVEL = {
  minimal: "info",
  low: "low",
  moderate: "medium",
  high: "high",
  critical: "critical",
};

// The load-bearing FR-5 reframe: "Assurance" alone can read as a verdict, so it is always
// paired with this — coverage-of-look, not a grade. Shared by the UI + the Markdown export so
// the two renderers cannot drift.
const ASSURANCE_FRAMING = "how complete this look was";

// The dual score — endpoint Exposure (magnitude, on the cool→warm gradient) beside Assurance
// ("how complete this look was" — instrumentation, never a grade), then the one-line
// interpretation. Metadata-only aggregates: it states coverage, never a verdict (FR-5).
// Returns "" when the scores are absent (pre-scoring builds), so the summary degrades cleanly.
export function renderDualScore(data) {
  if (!data.exposure || !data.assurance) return "";
  const level = EXPOSURE_BAND_LEVEL[data.exposure.band] ?? "info";
  const gaps = (data.coverage_gaps ?? []).length;
  const evasions = (data.evasion_signals ?? []).length;
  const coverage = [];
  if (gaps) coverage.push(`${gaps} coverage gap${plural(gaps)}`);
  if (evasions) coverage.push(`${evasions} evasion signal${plural(evasions)}`);
  const coverageNote = coverage.length ? ` · ${coverage.join(" · ")}` : "";
  return `<div class="dualscore">
    <div class="score exposure ${level}">
      ${levelMeter(level)}
      <span class="score-val num">${escapeHtml(data.exposure.score)}/100</span>
      <span class="score-lbl">Exposure · ${escapeHtml(data.exposure.band)}</span>
    </div>
    <div class="score assurance">
      <span class="score-val num">${escapeHtml(data.assurance.score)}/100</span>
      <span class="score-lbl">Assurance · ${escapeHtml(data.assurance.band)} — ${ASSURANCE_FRAMING}${escapeHtml(coverageNote)}</span>
    </div>
    ${data.interpretation ? `<p class="interpretation">${escapeHtml(data.interpretation)}</p>` : ""}
  </div>`;
}

// The summary screen body (after the scan, before the full inventory).
export function renderSummaryScreen(data) {
  const places = data.summary?.total_findings ?? 0;
  const tools = toolCount(data);
  const headline = computeHeadline(data);
  return `<p class="eyebrow">Scan complete</p>
    <h1 class="headline-lead">${escapeHtml(headline.lead)}</h1>
    <p class="headline-sub muted">${escapeHtml(headline.sub)}</p>
    ${renderDualScore(data)}
    <div class="statrow">
      <div class="stat"><span class="num">${places}</span><span class="lbl">place${plural(places)}</span></div>
      <div class="stat"><span class="num">${tools}</span><span class="lbl">tool${plural(tools)}</span></div>
      <div class="stat"><span class="num">${escapeHtml(humanSize(data.summary?.total_bytes ?? 0))}</span><span class="lbl">total</span></div>
    </div>
    ${renderAttentionChips(data, null)}
    ${offMachineNote(data)}
    <p class="disk muted">Disk encryption · ${escapeHtml(data.disk_encryption ?? "unknown")}${data.mode ? ` · Mode · ${escapeHtml(data.mode)}` : ""}</p>`;
}

function renderWarnings(data) {
  const w = data.warnings ?? [];
  if (!w.length) return "";
  const items = w
    .map((x) => `<li>${escapeHtml(x.path ? `${x.path}: ${x.reason}` : x.reason)}</li>`)
    .join("");
  return `<details class="warnings"><summary>${w.length} warning${plural(w.length)}</summary><ul>${items}</ul></details>`;
}

// The full results screen body.
export function renderResults(data, filter = null) {
  const places = data.summary?.total_findings ?? 0;
  const terms = osTerms(data.host?.os);
  const filterLine = filter
    ? `<p class="filterline muted">Showing <strong>${data.summary?.by_exposure?.[filter] ?? 0} ${escapeHtml(filter)}</strong> · <button class="show-all linkbtn" data-level="" type="button">show all</button></p>`
    : "";
  return `<button class="back-summary linkbtn" type="button">‹ Summary</button>
    <div class="results-head">
      <div>
        <p class="eyebrow">Footprint</p>
        <h1>What's on ${escapeHtml(terms.device)}</h1>
        <p class="muted">${places} place${plural(places)} · ${toolCount(data)} tool${plural(toolCount(data))}</p>
      </div>
      <div class="actions">
        <button class="export-open" type="button">Export</button>
        <button class="new-scan" type="button">New scan</button>
      </div>
    </div>
    ${renderDualScore(data)}
    ${renderAttentionChips(data, filter)}
    ${filterLine}
    <div class="groups">${renderGroups(data, filter)}</div>
    ${renderWarnings(data)}`;
}

// The finding detail (slide-over) body for a single finding. `os` (the report's `host.os`)
// selects the OS-aware amplifier notes + the "Reveal in …" file-manager label.
export function renderDetail(f, os) {
  if (!f) return "";
  const terms = osTerms(os);
  const facts = [humanSize(f.size_bytes)];
  if (f.file_count > 1) facts.push(`${f.file_count} files`);
  if (f.inspection?.line_count != null) facts.push(`${f.inspection.line_count} lines`);
  if (f.inspection?.row_count != null) facts.push(`${f.inspection.row_count} rows`);
  const mod = formatDate(f.modified_epoch_secs);
  if (mod) facts.push(`modified ${mod}`);
  if (f.evidence_class) facts.push(`evidence: ${f.evidence_class}`);

  const amps = (f.amplifiers ?? [])
    .map(
      (a) =>
        `<li class="amp-item ${ampKind(a)}"><span class="amp ${ampKind(a)}">${escapeHtml(ampLabel(a))}</span><span class="amp-note">${escapeHtml(ampNote(a, terms))}</span></li>`,
    )
    .join("");
  const guidance = (f.guidance ?? [])
    .map((g) => `<li><span class="arrow" aria-hidden="true">↳</span> ${escapeHtml(g)}</li>`)
    .join("");

  return `<div class="detail-head">${levelMeter(f.exposure_level)} ${levelTag(f.exposure_level)}</div>
    <h2>${escapeHtml(f.tool)}</h2>
    <p class="path mono">${escapeHtml(f.path)}</p>
    <p class="why-lead">${escapeHtml(f.why)}</p>
    <p class="facts mono muted">${escapeHtml(facts.join(" · "))}</p>
    ${amps ? `<h3 class="sub">Amplifiers</h3><ul class="amp-list">${amps}</ul>` : ""}
    ${guidance ? `<h3 class="sub">What you can do</h3><ul class="do-list">${guidance}</ul>` : ""}
    <div class="detail-foot">
      <button class="reveal primary" data-path="${escapeHtml(f.path)}" type="button">Reveal in ${escapeHtml(terms.fileManager)}</button>
    </div>`;
}

export function hasFindings(data) {
  return (data.summary?.total_findings ?? (data.findings ?? []).length) > 0;
}

// A plain-Markdown export of the report — metadata only, no conversation content.
// Plaintext output, so no HTML escaping; grouped by tool, highest-attention first.
export function reportToMarkdown(data) {
  const s = data.summary ?? {};
  const terms = osTerms(data.host?.os);
  const lines = [
    "# PromptDust report",
    "",
    `Read-only inventory of AI data on ${terms.device} — metadata only, nothing was changed.`,
    "",
    `- Places: ${s.total_findings ?? 0}`,
    `- Tools: ${Object.keys(s.by_tool ?? {}).length}`,
    `- Total size: ${humanSize(s.total_bytes ?? 0)}`,
    `- Disk encryption: ${data.disk_encryption ?? "unknown"}`,
    ...(data.mode ? [`- Scan mode: ${data.mode}`] : []),
    ...(data.exposure ? [`- Exposure: ${data.exposure.score}/100 (${data.exposure.band})`] : []),
    ...(data.assurance ? [`- Assurance: ${data.assurance.score}/100 (${data.assurance.band}) — ${ASSURANCE_FRAMING}`] : []),
    ...((data.coverage_gaps ?? []).length ? [`- Coverage gaps: ${data.coverage_gaps.length}`] : []),
    ...((data.evasion_signals ?? []).length ? [`- Evasion signals: ${data.evasion_signals.length}`] : []),
    ...(data.interpretation ? [`- ${data.interpretation}`] : []),
    "",
  ];

  const byTool = new Map();
  for (const f of data.findings ?? []) {
    if (!byTool.has(f.tool)) byTool.set(f.tool, []);
    byTool.get(f.tool).push(f);
  }
  const groups = [...byTool.entries()];
  for (const [, list] of groups) {
    list.sort((a, b) => exposureRank(a.exposure_level) - exposureRank(b.exposure_level));
  }
  groups.sort((a, b) => {
    const ra = a[1].reduce((m, f) => Math.min(m, exposureRank(f.exposure_level)), Infinity);
    const rb = b[1].reduce((m, f) => Math.min(m, exposureRank(f.exposure_level)), Infinity);
    return ra - rb;
  });

  for (const [tool, list] of groups) {
    lines.push(`## ${tool} (${list.length})`, "");
    for (const f of list) {
      const facts = [humanSize(f.size_bytes)];
      if (f.file_count > 1) facts.push(`${f.file_count} files`);
      if (f.evidence_class) facts.push(`evidence: ${f.evidence_class}`);
      lines.push(`- **${String(f.exposure_level).toUpperCase()}** \`${f.path}\` — ${facts.join(" · ")}`);
      if (f.why) lines.push(`  - ${f.why}`);
      const amps = (f.amplifiers ?? []).map(ampLabel);
      if (amps.length) lines.push(`  - Amplifiers: ${amps.join(", ")}`);
      for (const g of f.guidance ?? []) lines.push(`  - ${g}`);
    }
    lines.push("");
  }

  const w = data.warnings ?? [];
  if (w.length) {
    lines.push(`## Warnings (${w.length})`, "");
    for (const x of w) lines.push(`- ${x.path ? `${x.path}: ` : ""}${x.reason}`);
    lines.push("");
  }
  return lines.join("\n");
}
