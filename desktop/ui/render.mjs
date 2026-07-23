// Shared pure helpers for the PromptDust desktop UI. No DOM, no Tauri imports — every function
// maps data to a value/string, so it is unit-testable with `node --test`. The Panel + Inbox
// render layer (panel.mjs) and the app wiring (main.js) import from here. All dynamic values
// that reach HTML are escaped (the data is metadata-only, but we never trust-render).

export const EXPOSURE_ORDER = ["critical", "high", "medium", "low", "info"];

// Amplifier vocabulary: a plain-English note plus its "kind" —
//  offmachine = defines Critical (data left the machine); local = raises the level by one;
//  info = shown but never raises the level (backup_swept / large_growth). panel.mjs styles by
//  kind and fills `{device}`/`{backup}` placeholders from the OS-aware terms.
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
// (or absent) degrades to neutral, never-wrong wording.
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

// Numeric rank for a level (critical=0 … info=4; unknown sorts last) — the sort key for
// ordering findings/tools by attention.
export function exposureRank(level) {
  const idx = EXPOSURE_ORDER.indexOf(level);
  return idx === -1 ? EXPOSURE_ORDER.length : idx;
}

// The amplifier's display label (falls back to the de-underscored id for an unknown key).
function ampLabel(a) {
  return AMPLIFIERS[a]?.label ?? String(a).replaceAll("_", " ");
}

// A yyyy-mm-dd date from an epoch-seconds timestamp, or null when absent/malformed.
export function formatDate(epochSecs) {
  if (!epochSecs) return null;
  try {
    return new Date(epochSecs * 1000).toISOString().slice(0, 10);
  } catch {
    return null;
  }
}

// Endpoint Exposure bands (minimal…critical) mapped onto the UI's cool→warm level palette
// (info…critical) — the score reuses the same never-green gradient as the finding meters.
export const EXPOSURE_BAND_LEVEL = {
  minimal: "info",
  low: "low",
  moderate: "medium",
  high: "high",
  critical: "critical",
};

// The load-bearing FR-5 reframe: "Assurance" alone can read as a verdict, so the export always
// pairs it with this — coverage-of-look, not a grade. (The UI relabels it "Confidence".)
const ASSURANCE_FRAMING = "how complete this look was";

export function hasFindings(data) {
  return (data.summary?.total_findings ?? (data.findings ?? []).length) > 0;
}

// A plain-Markdown export of the report — metadata only, no conversation content. Plaintext
// output, so no HTML escaping; grouped by tool, highest-attention first.
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
