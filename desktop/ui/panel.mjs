// Pure render layer for the Panel + Inbox redesign (Design-v2). Like render.mjs, every
// function here maps data + UI state to an HTML string — no DOM, no Tauri — so it is
// unit-testable with `node --test`. main.js owns state, events, and the Tauri commands and
// re-renders the whole app from `renderApp(state)` on every change (the prototype's model).
//
// All dynamic values are HTML-escaped: the report is metadata-only, but we never trust-render.
// Visual language is ported from `.designs/v2/PromptDust - Panel.dc.html` (inline styles over
// the app's existing CSS variables) so the shipped app matches the approved prototype.

import {
  escapeHtml,
  humanSize,
  osTerms,
  formatDate,
  EXPOSURE_ORDER,
  EXPOSURE_BAND_LEVEL,
  AMPLIFIERS,
} from "./render.mjs";

// Level → gradient colour (CSS var), display name, and meter fill (5 = critical … 1 = info).
const LEVELS = {
  critical: { c: "var(--crit)", n: "Critical", f: 5 },
  high: { c: "var(--high)", n: "High", f: 4 },
  medium: { c: "var(--med)", n: "Medium", f: 3 },
  low: { c: "var(--low)", n: "Low", f: 2 },
  info: { c: "var(--info)", n: "Info", f: 1 },
};
const SHORT = { critical: "Crit", high: "High", medium: "Med", low: "Low", info: "Info" };

function levelInfo(level) {
  return LEVELS[level] ?? LEVELS.info;
}

// Exposure/confidence bands (minimal…critical / low…high) → the finding gradient level, so the
// scores and the meters share one never-green palette.
export function levelOfBand(band) {
  return EXPOSURE_BAND_LEVEL[band] ?? "info";
}

// The 5-segment meter, inline-styled to match the prototype. Filled up to the level's rank.
export function meter(level) {
  const { c, f } = levelInfo(level);
  const heights = [5, 7, 9, 11, 14];
  let bars = "";
  for (let i = 0; i < 5; i += 1) {
    bars += `<span style="width:3px;border-radius:1px;height:${heights[i]}px;background:${i < f ? c : "var(--meter-empty)"}"></span>`;
  }
  return `<span style="display:inline-flex;align-items:flex-end;gap:2px;height:14px">${bars}</span>`;
}

// The outlined, uppercase level tag.
export function tag(level) {
  const { c, n } = levelInfo(level);
  return `<span style="font-family:var(--font-mono);font-size:11px;font-weight:600;text-transform:uppercase;letter-spacing:.13em;padding:1px 8px;border-radius:4px;border:1px solid ${c};color:${c};white-space:nowrap">${escapeHtml(n)}</span>`;
}

// Per-finding UI state (read/pinned/flagged), keyed by finding path. `itemState` is the plain
// object returned by the backend `load_scan` (`item_state`); absent → all false.
export function findingState(itemState, path) {
  const st = itemState?.[path];
  return { read: !!st?.read, pinned: !!st?.pinned, flagged: !!st?.flagged };
}

function ampLabel(a) {
  return AMPLIFIERS[a]?.label ?? String(a).replaceAll("_", " ");
}
function ampKind(a) {
  return AMPLIFIERS[a]?.kind ?? "local";
}
function ampNote(a, terms) {
  return (AMPLIFIERS[a]?.note ?? "").replace(/\{(\w+)\}/g, (m, k) =>
    Object.hasOwn(terms, k) ? terms[k] : m,
  );
}

// Findings grouped by tool, each group sorted highest-attention first, groups ordered so the
// tool holding the highest-attention finding comes first. Preserves each finding's original
// index in `report.findings` (for detail lookup + persistence keyed by path).
export function groupsOf(findings) {
  const items = (findings ?? []).map((f, idx) => ({ f, idx }));
  const rank = (l) => {
    const i = EXPOSURE_ORDER.indexOf(l);
    return i === -1 ? EXPOSURE_ORDER.length : i;
  };
  const byTool = new Map();
  for (const it of items) {
    if (!byTool.has(it.f.tool)) byTool.set(it.f.tool, []);
    byTool.get(it.f.tool).push(it);
  }
  const groups = [...byTool.entries()];
  for (const [, list] of groups) list.sort((a, b) => rank(a.f.exposure_level) - rank(b.f.exposure_level));
  const minRank = (list) => list.reduce((m, { f }) => Math.min(m, rank(f.exposure_level)), Infinity);
  groups.sort((a, b) => minRank(a[1]) - minRank(b[1]));
  return groups;
}

// A deterministic, timezone-independent label for an RFC3339 timestamp (the report's
// `generated_at`, UTC). Pure so the Inbox rail is testable. e.g. "Oct 3 · 6:40 PM".
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
export function formatWhen(iso) {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return escapeHtml(iso ?? "");
  const mon = MONTHS[d.getUTCMonth()];
  const day = d.getUTCDate();
  let h = d.getUTCHours();
  const ampm = h < 12 ? "AM" : "PM";
  h = h % 12 || 12;
  const min = String(d.getUTCMinutes()).padStart(2, "0");
  return `${mon} ${day} · ${h}:${min} ${ampm} UTC`;
}

/* ---------------------------------------------------------------- icons */
const ICON = {
  chev: (open) => `<svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" style="transition:transform .15s;transform:rotate(${open ? 90 : 0}deg)"><path d="M6 4l4 4-4 4"/></svg>`,
  chevR: `<svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4l4 4-4 4"/></svg>`,
  chevL: `<svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M10 4l-4 4 4 4"/></svg>`,
  pin: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="var(--accent)" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M12 17v5"/><path d="M9 10.8a2 2 0 0 1-1.1 1.8l-1.8.9A2 2 0 0 0 5 15.2V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.8a2 2 0 0 0-1.1-1.7l-1.8-.9A2 2 0 0 1 15 10.8V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/></svg>',
  flag: '<svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="var(--high)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M4 2v12"/><path d="M4 2.8h8l-1.6 2.7L12 8.2H4" fill="var(--high)" stroke="none"/></svg>',
  kebab: '<svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><circle cx="8" cy="3.5" r="1.3"/><circle cx="8" cy="8" r="1.3"/><circle cx="8" cy="12.5" r="1.3"/></svg>',
  info: '<svg viewBox="0 0 18 18" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="9" cy="9" r="7"/><path d="M9 8v4.5" stroke-linecap="round"/><circle cx="9" cy="5.6" r=".2" stroke-width="1.6"/></svg>',
  gear: '<svg viewBox="0 0 20 20" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="10" cy="10" r="2.6"/><path d="M10 2.5v2M10 15.5v2M2.5 10h2M15.5 10h2M4.7 4.7l1.4 1.4M13.9 13.9l1.4 1.4M15.3 4.7l-1.4 1.4M6.1 13.9l-1.4 1.4"/></svg>',
  share: '<svg viewBox="0 0 20 20" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13V3"/><path d="M6.5 6.5L10 3l3.5 3.5"/><path d="M5 10v6h10v-6"/></svg>',
};

const PRINCIPLES = [
  ["Read-only", "Never modifies, moves, or deletes a file."],
  ["Local-only", "Nothing leaves {device} — no accounts, no network."],
  ["Inventory, not a verdict", "Reports what's here, never a pass/fail."],
  ["Metadata-only", "Never reads a word of your conversations."],
];

function menuItem(attr, label, danger = false) {
  return `<button ${attr} data-menu style="display:flex;align-items:center;gap:9px;width:100%;text-align:left;padding:8px 14px;border:none;background:transparent;cursor:pointer;font-size:13px;color:${danger ? "var(--crit)" : "var(--fg)"}">${escapeHtml(label)}</button>`;
}

/* ---------------------------------------------------------------- inbox rail */
// `index` is the backend `list_scans` array: { run_id, ran_at, exposure:{score,band},
// confidence:{score,band}, headline, trace_count, unread }.
export function renderInbox({ index = [], selRunId = null, open = false, scanning = false } = {}) {
  if (!open) {
    const unread = index.some((s) => s.unread);
    return `<div style="width:52px;border-right:1px solid var(--border);display:flex;flex-direction:column;align-items:center;flex-shrink:0;padding:12px 0;gap:16px">
      <button data-inbox-toggle title="Show scans" style="display:inline-flex;padding:6px;color:var(--muted);background:transparent;border:none;cursor:pointer">${ICON.chevR}</button>
      <div style="writing-mode:vertical-rl;font-family:var(--font-mono);font-size:11px;letter-spacing:.2em;text-transform:uppercase;color:var(--faint)">Scans · ${index.length}</div>
      ${unread ? `<span style="width:7px;height:7px;border-radius:50%;background:var(--accent)"></span>` : ""}
    </div>`;
  }
  const scanRow = scanning
    ? `<div style="display:flex;align-items:center;gap:10px;padding:13px 16px;border-bottom:1px solid var(--border);background:var(--card)"><span class="pd-spin" style="width:14px;height:14px;border:2px solid var(--border);border-top-color:var(--accent);border-radius:50%;display:inline-block"></span><span style="font-family:var(--font-mono);font-size:11px;color:var(--muted)">Scanning…</span></div>`
    : "";
  const rows = index
    .map((s) => {
      const active = s.run_id === selRunId;
      const level = levelOfBand(s.exposure?.band);
      const c = levelInfo(level).c;
      const dot = s.unread ? `<span style="width:7px;height:7px;border-radius:50%;background:var(--accent);flex-shrink:0"></span>` : "";
      const exp = s.exposure ? `Exp ${escapeHtml(s.exposure.score)} · ${escapeHtml(s.exposure.band)}` : "Exp —";
      return `<button data-run="${escapeHtml(s.run_id)}" style="display:flex;flex-direction:column;gap:6px;width:100%;text-align:left;padding:13px 16px;border:none;border-left:3px solid ${active ? "var(--accent)" : "transparent"};background:${active ? "var(--card)" : "transparent"};cursor:pointer;border-bottom:1px solid var(--border)">
        <span style="display:flex;align-items:center;gap:7px"><span style="font-family:var(--font-mono);font-size:11px;letter-spacing:.06em;text-transform:uppercase;color:${active ? "var(--fg)" : "var(--muted)"};flex:1">${escapeHtml(formatWhen(s.ran_at))}</span>${dot}</span>
        <span style="font-family:var(--font-display);font-size:16px;letter-spacing:-.01em;color:var(--fg)">${escapeHtml(s.headline ?? "")}</span>
        <span style="display:flex;align-items:center;gap:8px">${meter(level)}<span style="font-family:var(--font-mono);font-size:10px;letter-spacing:.1em;text-transform:uppercase;color:${c}">${exp}</span><span style="font-family:var(--font-mono);font-size:10px;color:var(--faint)">${escapeHtml(s.trace_count ?? 0)} traces</span></span>
      </button>`;
    })
    .join("");
  return `<div style="width:242px;border-right:1px solid var(--border);display:flex;flex-direction:column;flex-shrink:0">
    <div style="display:flex;align-items:center;justify-content:space-between;padding:14px 12px 10px 14px"><button data-inbox-toggle title="Hide scans" style="display:flex;align-items:center;gap:7px;padding:2px;background:transparent;border:none;cursor:pointer"><span style="color:var(--faint);display:inline-flex">${ICON.chevL}</span><span style="font-family:var(--font-mono);font-size:11px;letter-spacing:.2em;text-transform:uppercase;color:var(--faint)">Scans</span></button><button class="primary" data-newscan style="font-size:12px;padding:5px 11px" type="button">New scan</button></div>
    <div style="overflow:auto;flex:1">${scanRow}${rows}</div>
  </div>`;
}

/* ---------------------------------------------------------------- ribbon */
export function renderDistribution(byExposure = {}, total = 0) {
  const order = EXPOSURE_ORDER.filter((l) => byExposure[l]);
  const W = 216;
  const bar = order
    .map((l) => `<span title="${escapeHtml(levelInfo(l).n)} · ${escapeHtml(byExposure[l])}" style="flex:${byExposure[l]};background:${levelInfo(l).c}"></span>`)
    .join("");
  const counts = order
    .map((l) => `<span style="flex:${byExposure[l]};min-width:22px;text-align:center;font-family:var(--font-display);font-size:15px;line-height:1;color:${levelInfo(l).c}">${escapeHtml(byExposure[l])}</span>`)
    .join("");
  const labels = order
    .map((l) => `<span style="flex:${byExposure[l]};min-width:22px;text-align:center;font-family:var(--font-mono);font-size:9px;letter-spacing:.05em;text-transform:uppercase;color:var(--faint);overflow:hidden;white-space:nowrap">${SHORT[l]}</span>`)
    .join("");
  return `<div style="display:flex;flex-direction:column;gap:6px;align-items:flex-end">
    <span style="font-family:var(--font-mono);font-size:10px;letter-spacing:.14em;text-transform:uppercase;color:var(--faint)">Distribution · ${escapeHtml(total)} traces</span>
    <div style="display:flex;width:${W}px;gap:3px;align-items:flex-end">${counts}</div>
    <div style="display:flex;height:8px;width:${W}px;border-radius:4px;overflow:hidden;gap:3px">${bar}</div>
    <div style="display:flex;width:${W}px;gap:3px">${labels}</div>
  </div>`;
}

function scoreBlock(val, lbl, band, level, accent) {
  return `<div data-testid="score-${escapeHtml(String(lbl).toLowerCase())}"><div style="display:flex;align-items:flex-end;gap:8px">${accent ? "" : meter(level)}<span style="font-family:var(--font-display);font-size:24px;line-height:1;${accent ? "color:var(--accent)" : ""}">${escapeHtml(val)}</span><span style="font-family:var(--font-mono);font-size:11px;color:var(--faint);padding-bottom:3px">/100</span></div><div style="font-family:var(--font-mono);font-size:10px;letter-spacing:.14em;text-transform:uppercase;color:var(--faint);margin-top:5px">${escapeHtml(lbl)} · ${escapeHtml(band)}</div></div>`;
}

// The score ribbon: Exposure (magnitude, on the gradient) + Confidence (report `assurance`,
// relabeled — trust in the reading, not a grade), and the Distribution mini-chart.
export function renderRibbon(report) {
  const by = report.summary?.by_exposure ?? {};
  const total = report.summary?.total_findings ?? 0;
  const exposure = report.exposure ?? { score: "—", band: "—" };
  const confidence = report.assurance ?? { score: "—", band: "—" };
  const level = levelOfBand(exposure.band);
  return `<div style="display:flex;align-items:center;justify-content:space-between;padding:12px 20px;border-bottom:1px solid var(--border)"><div style="display:flex;gap:26px;align-items:flex-end">${scoreBlock(exposure.score, "Exposure", exposure.band, level, false)}${scoreBlock(confidence.score, "Confidence", confidence.band, null, true)}</div>${renderDistribution(by, total)}</div>`;
}

/* ---------------------------------------------------------------- findings list */
function findingRow(f, idx, ui) {
  const active = f.path === ui.selPath;
  const c = levelInfo(f.exposure_level).c;
  const st = findingState(ui.itemState, f.path);
  const file = f.path.split("/").pop();
  const dir = f.path.slice(0, f.path.length - file.length);
  const marks = `${st.pinned ? ICON.pin : ""}${st.flagged ? ICON.flag : ""}`;
  const udot = st.read ? "" : `<span style="width:6px;height:6px;border-radius:50%;background:var(--accent);flex-shrink:0"></span>`;
  return `<button data-find="${idx}" data-file="${escapeHtml(file)}" data-pinned="${st.pinned}" data-flagged="${st.flagged}" data-read="${st.read}" style="display:flex;flex-direction:column;gap:3px;width:100%;text-align:left;padding:9px 16px 9px 34px;border:none;border-left:3px solid ${active ? c : "transparent"};background:${active ? "var(--inset)" : "transparent"};cursor:pointer"><span style="display:flex;align-items:center;gap:7px;width:100%">${meter(f.exposure_level)}${udot}<span style="font-family:var(--font-mono);font-size:12px;color:var(--fg);flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:${st.read ? 400 : 600}">${escapeHtml(file)}</span>${marks}<span style="font-family:var(--font-mono);font-size:11px;color:var(--muted)">${escapeHtml(humanSize(f.size_bytes))}</span></span><span style="font-family:var(--font-mono);font-size:10px;color:var(--faint);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;padding-left:19px">${escapeHtml(dir)}</span></button>`;
}

function fchip(label, n, lvl, ui) {
  const active = (ui.filter ?? null) === (lvl ?? null);
  const c = lvl ? levelInfo(lvl).c : "var(--accent)";
  return `<button data-flevel="${escapeHtml(lvl ?? "")}" style="font-family:var(--font-mono);font-size:10px;letter-spacing:.08em;text-transform:uppercase;padding:3px 9px;border:1px solid ${active ? c : "var(--border)"};border-radius:999px;background:${active ? "var(--inset)" : "transparent"};color:${lvl ? c : "var(--muted)"};cursor:pointer;white-space:nowrap;display:inline-flex;gap:5px">${escapeHtml(label)}<span style="color:var(--fg)">${escapeHtml(n)}</span></button>`;
}

export function renderList(report, ui) {
  const findings = report.findings ?? [];
  // finding -> its ORIGINAL index in report.findings. data-find must be this index (not the
  // position within a filtered/grouped subset), because selectFinding looks it up as
  // report.findings[idx]. Grouping/filtering must not renumber it.
  const idxOf = new Map(findings.map((f, i) => [f, i]));
  const filter = ui.filter ?? null;
  const shown = filter ? findings.filter((f) => f.exposure_level === filter) : findings;
  const expanded = ui.expanded ?? new Set();

  const groupsHtml =
    groupsOf(shown)
      .map(([tool, items]) => {
        const open = expanded.has(tool);
        const unread = items.filter(({ f }) => !findingState(ui.itemState, f.path).read).length;
        const bytes = items.reduce((n, { f }) => n + (Number(f.size_bytes) || 0), 0);
        const badge = unread
          ? `<span style="min-width:16px;height:16px;padding:0 4px;border-radius:8px;background:var(--accent);color:var(--on-accent);font-family:var(--font-mono);font-size:10px;display:inline-flex;align-items:center;justify-content:center">${unread}</span>`
          : "";
        const head = `<button data-group="${escapeHtml(tool)}" style="display:flex;align-items:center;gap:9px;width:100%;text-align:left;padding:11px 16px;border:none;background:transparent;cursor:pointer;border-bottom:1px solid var(--border)"><span style="color:var(--faint);display:inline-flex">${ICON.chev(open)}</span><span style="font-family:var(--font-display);font-size:15px;letter-spacing:-.01em;color:var(--fg);flex:1">${escapeHtml(tool)}</span>${badge}<span style="font-family:var(--font-mono);font-size:11px;color:var(--faint)">${items.length} · ${escapeHtml(humanSize(bytes))}</span></button>`;
        if (!open) return head;
        return head + items.map(({ f }) => findingRow(f, idxOf.get(f), ui)).join("");
      })
      .join("") ||
    `<p style="font-family:var(--font-mono);font-size:12px;color:var(--faint);padding:20px 16px">Nothing at this level.</p>`;

  const by = report.summary?.by_exposure ?? {};
  const total = report.summary?.total_findings ?? 0;
  const chips = [
    fchip("All", total, null, ui),
    ...EXPOSURE_ORDER.filter((l) => by[l]).map((l) => fchip(levelInfo(l).n, by[l], l, ui)),
  ].join("");
  const filterRow = `<div style="display:flex;flex-wrap:wrap;gap:6px;padding:0 16px 12px;border-bottom:1px solid var(--border)">${chips}</div>`;

  const menu = ui.listMenuOpen
    ? `<div data-menu style="position:absolute;right:12px;top:40px;width:200px;background:var(--card);border:1px solid var(--border-strong);border-radius:10px;box-shadow:var(--shadow-window);padding:5px 0;z-index:6">${menuItem('data-listaction="markread"', "Mark all as read")}${menuItem('data-listaction="expand"', "Expand all")}${menuItem('data-listaction="collapse"', "Collapse all")}<div style="height:1px;background:var(--border);margin:5px 0"></div>${menuItem('data-listaction="export"', "Export report…")}</div>`
    : "";

  const pinned = findings.filter((f) => findingState(ui.itemState, f.path).pinned);
  const toolCount = new Set(findings.map((f) => f.tool)).size;
  const pinChip = pinned.length
    ? `<button data-pins-toggle data-menu title="Pinned items" style="display:inline-flex;align-items:center;gap:5px;padding:3px 9px 3px 7px;border:1px solid ${ui.pinsOpen ? "var(--accent)" : "var(--border)"};border-radius:999px;background:${ui.pinsOpen ? "var(--accent-soft)" : "var(--card)"};cursor:pointer;color:var(--accent);font-family:var(--font-mono);font-size:11px;letter-spacing:.06em">${ICON.pin}${pinned.length}</button>`
    : "";
  const pinsPop =
    ui.pinsOpen && pinned.length
      ? `<div data-menu style="position:absolute;left:16px;top:44px;width:288px;background:var(--card);border:1px solid var(--border-strong);border-radius:10px;box-shadow:var(--shadow-window);padding:5px 0;z-index:7"><p style="font-family:var(--font-mono);font-size:10px;letter-spacing:.14em;text-transform:uppercase;color:var(--faint);margin:2px 0 4px;padding:0 14px">Pinned</p>${pinned
          .map((f) => {
            const idx = findings.indexOf(f);
            const file = f.path.split("/").pop();
            return `<button data-find="${idx}" style="display:flex;align-items:center;gap:8px;width:100%;text-align:left;padding:8px 14px;border:none;background:transparent;cursor:pointer">${meter(f.exposure_level)}<span style="font-family:var(--font-mono);font-size:12px;color:var(--fg);flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${escapeHtml(file)}</span><span style="font-size:11px;color:var(--muted);white-space:nowrap">${escapeHtml(f.tool)}</span></button>`;
          })
          .join("")}</div>`
      : "";

  return `<div style="width:330px;border-right:1px solid var(--border);overflow:auto;flex-shrink:0;position:relative">
    <div style="display:flex;align-items:center;justify-content:space-between;gap:10px;padding:12px 12px 10px 16px"><div style="display:flex;align-items:center;gap:10px;min-width:0"><span style="font-family:var(--font-mono);font-size:11px;letter-spacing:.2em;text-transform:uppercase;color:var(--faint);white-space:nowrap">${total} traces · ${toolCount} tools</span>${pinChip}</div><button data-listmenu-toggle data-menu style="display:inline-flex;padding:4px 6px;color:var(--muted);background:transparent;border:none;cursor:pointer" type="button">${ICON.kebab}</button></div>
    ${filterRow}
    ${pinsPop}${menu}${groupsHtml}
  </div>`;
}

/* ---------------------------------------------------------------- detail pane */
export function renderDetail(report, ui) {
  const findings = report.findings ?? [];
  const f = findings.find((x) => x.path === ui.selPath);
  if (!f) return `<div style="flex:1;background:var(--card)"></div>`;
  const terms = osTerms(report.host?.os);
  const st = findingState(ui.itemState, f.path);
  const amps = (f.amplifiers ?? [])
    .map((a) => {
      const kind = ampKind(a);
      const c = kind === "offmachine" ? "var(--crit)" : kind === "info" ? "var(--faint)" : "var(--muted)";
      return `<li style="display:flex;gap:10px;margin-bottom:11px"><span style="font-family:var(--font-mono);font-size:10px;text-transform:uppercase;letter-spacing:.1em;padding:1px 6px;border-radius:2px;border:1px solid ${kind === "offmachine" ? "currentColor" : "var(--border)"};color:${c};align-self:flex-start;white-space:nowrap">${escapeHtml(ampLabel(a))}</span><span style="color:var(--muted);font-size:13px">${escapeHtml(ampNote(a, terms))}</span></li>`;
    })
    .join("");
  const guide = (f.guidance ?? [])
    .map((g) => `<li style="display:flex;gap:8px;color:var(--muted);font-size:14px;margin-bottom:8px"><span style="color:var(--accent);flex-shrink:0">↳</span><span>${escapeHtml(g)}</span></li>`)
    .join("");
  const facts = [humanSize(f.size_bytes), `${f.file_count} file${f.file_count === 1 ? "" : "s"}`];
  const mod = formatDate(f.modified_epoch_secs);
  if (mod) facts.push(mod);

  const share = ui.shareOpen
    ? `<div data-menu style="position:absolute;right:0;bottom:44px;width:216px;background:var(--card);border:1px solid var(--border-strong);border-radius:10px;box-shadow:var(--shadow-window);padding:6px 0;z-index:6">${menuItem('data-share="copy"', "Copy summary")}<div style="height:1px;background:var(--border);margin:5px 0"></div>${menuItem('data-share="native"', "Share…")}</div>`
    : "";
  const dmenu = ui.detailMenuOpen
    ? `<div data-menu style="position:absolute;right:26px;top:52px;width:186px;background:var(--card);border:1px solid var(--border-strong);border-radius:10px;box-shadow:var(--shadow-window);padding:5px 0;z-index:6">${menuItem('data-detailaction="unread"', "Mark as unread")}${menuItem('data-detailaction="pin"', st.pinned ? "Unpin" : "Pin")}${menuItem('data-detailaction="flag"', st.flagged ? "Remove flag" : "Flag")}${menuItem('data-detailaction="copy"', "Copy path")}</div>`
    : "";
  const marks = `${st.pinned ? `<span style="display:inline-flex">${ICON.pin}</span>` : ""}${st.flagged ? `<span style="display:inline-flex">${ICON.flag}</span>` : ""}`;

  return `<div style="flex:1;overflow:auto;background:var(--card);display:flex;flex-direction:column;position:relative">
    <div style="padding:22px 26px;overflow:auto;flex:1">
      <div style="display:flex;align-items:center;gap:10px;margin-bottom:16px">${meter(f.exposure_level)}${tag(f.exposure_level)}${marks}<span style="flex:1"></span><button data-detailmenu-toggle data-menu style="display:inline-flex;padding:4px 6px;color:var(--muted);background:transparent;border:none;cursor:pointer" type="button">${ICON.kebab}</button></div>
      <h2 style="font-family:var(--font-display);font-size:26px;letter-spacing:-.02em;margin:0 0 5px">${escapeHtml(f.tool)}</h2>
      <p style="font-family:var(--font-mono);font-size:12px;color:var(--muted);word-break:break-all;margin:0 0 18px">${escapeHtml(f.path)}</p>
      <p style="font-size:15px;line-height:1.5;margin:0 0 6px;color:var(--fg)">${escapeHtml(f.why)}</p>
      <p style="font-family:var(--font-mono);font-size:12px;color:var(--faint);margin:0 0 22px">${escapeHtml(facts.join(" · "))}</p>
      ${amps ? `<p style="font-family:var(--font-mono);font-size:11px;letter-spacing:.14em;text-transform:uppercase;color:var(--faint);margin:0 0 11px">Amplifying it</p><ul style="list-style:none;margin:0 0 22px;padding:0">${amps}</ul>` : ""}
      ${guide ? `<p style="font-family:var(--font-mono);font-size:11px;letter-spacing:.14em;text-transform:uppercase;color:var(--faint);margin:0 0 11px">What you can do</p><ul style="list-style:none;margin:0;padding:0">${guide}</ul>` : ""}
    </div>
    <div style="position:relative;border-top:1px solid var(--border);padding:14px 26px;display:flex;align-items:center;gap:10px">
      <button class="primary" data-reveal type="button">Reveal in ${escapeHtml(terms.fileManager)}</button>
      <button data-share-toggle data-menu type="button" style="display:flex;align-items:center;gap:7px">${ICON.share}Share</button>
      ${share}
    </div>
    ${dmenu}
  </div>`;
}

/* ---------------------------------------------------------------- appbar + workspace */
export function renderAppbar(ui) {
  const terms = osTerms(ui.os);
  const info = ui.infoOpen
    ? `<div data-menu style="position:absolute;right:16px;top:46px;width:280px;background:var(--card);border:1px solid var(--border-strong);border-radius:10px;box-shadow:var(--shadow-window);padding:14px 16px;z-index:8"><p style="font-family:var(--font-mono);font-size:10px;letter-spacing:.16em;text-transform:uppercase;color:var(--faint);margin:0 0 10px">How PromptDust works</p>${PRINCIPLES.map(([h, b]) => `<div style="font-size:12px;color:var(--muted);line-height:1.45;margin-bottom:8px"><strong style="color:var(--fg);font-weight:600">${escapeHtml(h)}.</strong> ${escapeHtml(b.replace("{device}", terms.device))}</div>`).join("")}</div>`
    : "";
  return `<header class="appbar" style="padding:6px 20px;position:relative;box-sizing:border-box"><div class="brand"><span class="glyph"><img src="icon.svg" style="height:34px;width:34px" alt=""></span><span class="wordmark">PromptDust</span></div><div class="appbar-actions"><button class="icon-btn" data-settings-toggle type="button" aria-label="Settings">${ICON.gear}</button><button class="icon-btn" data-info-toggle data-menu type="button" aria-label="How it works">${ICON.info}</button><button class="icon-btn" data-theme-toggle type="button" aria-label="Toggle theme">◐</button></div>${info}</header>`;
}

export function renderWorkspace(report, index, ui) {
  return `<div style="position:relative;display:flex;flex-direction:column;height:100vh">${renderAppbar(ui)}${renderRibbon(report)}<div style="display:flex;flex:1;min-height:0">${renderInbox({ index, selRunId: ui.selRunId, open: ui.inboxOpen, scanning: ui.scanning })}${renderList(report, ui)}${renderDetail(report, ui)}</div></div>`;
}

// A transient status toast, rendered app-wide (fixed to the viewport) so it shows on every
// screen — the welcome/consent flow toasts too, not just the workspace.
export function renderToast(ui) {
  if (!ui.toast) return "";
  return `<div data-testid="toast" style="position:fixed;left:50%;bottom:24px;transform:translateX(-50%);background:var(--fg);color:var(--bg);font-size:13px;padding:9px 16px;border-radius:8px;box-shadow:var(--shadow-window);z-index:50;white-space:nowrap">${escapeHtml(ui.toast)}</div>`;
}

/* ---------------------------------------------------------------- full-screen states */
function centered(ui, inner) {
  return `<div style="position:relative;display:flex;flex-direction:column;min-height:100vh">${renderAppbar(ui)}<div style="flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center;padding:40px 48px;gap:14px">${inner}</div></div>`;
}

export function renderWelcome(ui) {
  const device = osTerms(ui.os).device;
  const promises = ["Looks, never touches", `Never leaves ${device}`, "Never reads a word"]
    .map((p) => `<span style="font-family:var(--font-mono);font-size:12px;text-transform:uppercase;letter-spacing:.13em;color:var(--muted);border:1px solid var(--border);border-radius:4px;padding:4px 9px">${escapeHtml(p)}</span>`)
    .join("");
  return `<div style="position:relative;display:flex;flex-direction:column;min-height:100vh">${renderAppbar(ui)}
    <div style="flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center;padding:40px 48px 84px">
      <img src="icon.svg" width="84" height="84" alt="">
      <h1 style="font-family:var(--font-display);font-size:52px;letter-spacing:-.02em;margin:18px 0 4px">PromptDust</h1>
      <p style="font-family:var(--font-display);font-style:italic;color:var(--accent);font-size:22px;margin:0 0 22px">the dust your prompts leave behind</p>
      <div style="display:flex;flex-wrap:wrap;gap:8px;justify-content:center;margin:0 0 26px">${promises}</div>
      <button class="primary" data-scan-start style="padding:13px 30px;font-size:16px" type="button">Scan ${escapeHtml(device)}</button>
      <p style="color:var(--faint);font-size:13px;max-width:30rem;margin:16px 0 0">A footprint mapper — no delete button. It only shows you and points the way.</p>
    </div>
    <div style="position:absolute;left:0;right:0;bottom:0;display:flex;align-items:center;justify-content:center;gap:6px;padding:14px;border-top:1px solid var(--border);font-size:12px;color:var(--faint)">
      <span>Help improve PromptDust?</span>
      <button data-open-consent type="button" style="background:none;border:none;padding:0;color:var(--accent);cursor:pointer;font-size:12px;text-decoration:underline">Anonymous sharing</button>
    </div>
  </div>`;
}

export function renderScanning(ui) {
  return centered(ui, `
    <div style="width:88px;height:88px;position:relative;margin-bottom:8px"><span class="pd-spin" style="position:absolute;inset:0;border:2px solid var(--border);border-top-color:var(--accent);border-radius:50%"></span></div>
    <p style="font-family:var(--font-mono);font-size:14px;color:var(--muted);margin:0">Reading known locations on ${escapeHtml(osTerms(ui.os).device)}…</p>
    <div style="width:220px;height:3px;background:var(--inset);border-radius:999px;overflow:hidden;margin:6px 0"><span class="pd-indet" style="display:block;height:100%;width:42%;background:var(--accent);border-radius:999px"></span></div>
  `);
}

export function renderEmpty(ui) {
  return centered(ui, `
    <img src="icon.svg" width="72" height="72" alt="" style="opacity:.45;margin-bottom:6px">
    <h1 style="font-family:var(--font-display);font-size:34px;letter-spacing:-.02em;margin:0">No data from known AI tools found</h1>
    <p style="color:var(--muted);font-size:15px;max-width:32rem;margin:0">Only known AI tools were checked.</p>
    <button class="primary" data-scan-start style="margin-top:6px" type="button">Scan again</button>
  `);
}

export function renderPermission(ui) {
  return centered(ui, `
    <span style="display:inline-flex;color:var(--accent);margin-bottom:4px"><svg viewBox="0 0 24 24" width="44" height="44" fill="none"><circle cx="8" cy="9" r="4.5" stroke="currentColor" stroke-width="2"></circle><path d="M11 12 L20 21 M17 18 l2 -2 M19 20 l2 -2" stroke="currentColor" stroke-width="2" stroke-linecap="round"></path></svg></span>
    <h1 style="font-family:var(--font-display);font-size:32px;letter-spacing:-.02em;margin:0">Permission needed</h1>
    <p style="color:var(--muted);font-size:15px;max-width:32rem;margin:0">${escapeHtml(ui.permMsg || `PromptDust needs access to your files.`)}</p>
    <div style="display:flex;gap:10px;margin-top:8px"><button class="primary" data-scan-start type="button">Try again</button><button data-go-welcome type="button">Back</button></div>
  `);
}

/* ---------------------------------------------------------------- overlays */
function sw(on) {
  return `<span style="display:inline-flex;width:38px;height:22px;border-radius:999px;background:${on ? "var(--accent)" : "var(--border-strong)"};position:relative;flex-shrink:0"><span style="position:absolute;top:2px;left:${on ? 18 : 2}px;width:18px;height:18px;border-radius:50%;background:#fff"></span></span>`;
}
function modal(inner, w = 400) {
  return `<div style="position:absolute;inset:0;z-index:40"><div data-close-ov style="position:absolute;inset:0;background:rgba(6,12,14,.42)"></div><div data-menu style="position:absolute;top:50%;left:50%;transform:translate(-50%,-50%);width:${w}px;max-width:92%;max-height:86%;overflow:auto;background:var(--card);border:1px solid var(--border-strong);border-radius:14px;box-shadow:var(--shadow-window)">${inner}</div></div>`;
}
function eyebrow(t) {
  return `<p style="font-family:var(--font-mono);font-size:10px;letter-spacing:.16em;text-transform:uppercase;color:var(--faint);margin:0 0 6px">${escapeHtml(t)}</p>`;
}
function pre(t) {
  return `<pre style="font-family:var(--font-mono);font-size:11px;line-height:1.55;color:var(--fg);background:var(--inset);border:1px solid var(--border);border-radius:8px;padding:14px;margin:0;white-space:pre-wrap;word-break:break-word">${escapeHtml(t)}</pre>`;
}

export function renderConsent() {
  const reads = [
    "Counts of findings by attention level",
    "Tool, database & schema versions",
    "OS and architecture (e.g. macOS · arm64)",
    "Redacted, path-free error notes",
  ]
    .map((r) => `<li style="margin:5px 0">${escapeHtml(r)}</li>`)
    .join("");
  return modal(
    `<div style="padding:24px">
    ${eyebrow("Optional · off by default")}
    <h2 style="font-family:var(--font-display);font-size:24px;letter-spacing:-.02em;margin:0 0 14px">Help improve PromptDust?</h2>
    <p style="font-family:var(--font-mono);font-size:10px;letter-spacing:.14em;text-transform:uppercase;color:var(--faint);margin:0 0 6px">What's sent</p>
    <ul style="margin:0 0 14px;padding-left:18px;color:var(--muted);font-size:13px">${reads}</ul>
    <div style="background:var(--accent-soft);border-radius:8px;padding:12px 14px;font-size:13px;color:var(--fg);margin:0 0 18px">No paths, names, or content. New random ID per send — no identifier. Honors <span style="font-family:var(--font-mono)">DO_NOT_TRACK</span>.</div>
    <div style="display:flex;gap:10px"><button data-consent="no" type="button" style="flex:1;padding:10px">Not now</button><button data-consent="yes" class="primary" type="button" style="flex:1;padding:10px">Turn on sharing</button></div>
  </div>`,
    420,
  );
}

function settingRow(title, desc, control) {
  return `<div style="display:flex;align-items:center;gap:14px;padding:15px 0;border-bottom:1px solid var(--border)"><div style="flex:1"><div style="font-size:14px;font-weight:600;color:var(--fg)">${escapeHtml(title)}</div>${desc ? `<div style="font-size:12px;color:var(--muted);margin-top:2px;line-height:1.45">${desc}</div>` : ""}</div>${control}</div>`;
}
export function renderSettings(ui) {
  const envOff = ui.suppressedByEnv;
  const teleDesc = envOff
    ? `Forced off by the environment (<span style="font-family:var(--font-mono)">DO_NOT_TRACK</span>).`
    : `<button data-tele-preview type="button" style="background:none;border:none;padding:0;color:var(--accent);cursor:pointer;font-size:12px;text-decoration:underline">Preview what's sent</button>`;
  const tele = settingRow(
    "Anonymous usage sharing",
    teleDesc,
    envOff
      ? sw(false)
      : `<button data-tele-toggle type="button" style="background:none;border:none;padding:0;cursor:pointer">${sw(!!ui.telemetry)}</button>`,
  );
  const crash = settingRow(
    "Crash reporting",
    "Local & redacted. Off with <span style=\"font-family:var(--font-mono)\">DO_NOT_TRACK</span>.",
    "",
  );
  const diag = settingRow(
    "Diagnostics bundle",
    "",
    `<button data-diag-open type="button" style="font-size:13px">Create…</button>`,
  );
  const updates = settingRow(
    "Check for updates",
    ui.updateStatus
      ? escapeHtml(ui.updateStatus)
      : "PromptDust only checks when you ask, never on its own.",
    `<button data-check-updates type="button" style="font-size:13px">Check</button>`,
  );
  return modal(
    `<div style="padding:22px 24px">
    <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:6px">${eyebrow("Settings · Privacy")}<button data-close-ov type="button" style="background:none;border:none;color:var(--muted);font-size:20px;padding:0;cursor:pointer">×</button></div>
    <h2 style="font-family:var(--font-display);font-size:22px;letter-spacing:-.02em;margin:0 0 6px">Feedback &amp; privacy</h2>
    ${tele}${crash}${diag}${updates}
    <p style="font-size:12px;color:var(--faint);margin:14px 0 0">The scan never uses the network. Honors <span style="font-family:var(--font-mono)">DO_NOT_TRACK</span>.</p>
  </div>`,
    460,
  );
}

export function renderTelePreview(ui) {
  const body = ui.telePreviewText || "Loading…";
  return modal(
    `<div style="padding:22px 24px">
    <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:8px">${eyebrow("Telemetry · exactly what's sent")}<button data-close-ov type="button" style="background:none;border:none;color:var(--muted);font-size:20px;padding:0;cursor:pointer">×</button></div>
    ${pre(body)}
  </div>`,
    460,
  );
}

export function renderDiag(ui) {
  const body = ui.diagText || "Loading…";
  return modal(
    `<div style="padding:22px 24px">
    <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:8px">${eyebrow("Diagnostics bundle · inspect before sharing")}<button data-close-ov type="button" style="background:none;border:none;color:var(--muted);font-size:20px;padding:0;cursor:pointer">×</button></div>
    ${pre(body)}
    <div style="display:flex;align-items:center;gap:10px;margin-top:16px"><button data-diag-share class="primary" type="button">Share bundle…</button><button data-close-ov type="button">Close</button></div>
  </div>`,
    520,
  );
}

export function renderOverlays(ui) {
  let o = "";
  if (ui.consentOpen) o += renderConsent();
  if (ui.settingsOpen) o += renderSettings(ui);
  if (ui.telePreviewOpen) o += renderTelePreview(ui);
  if (ui.diagOpen) o += renderDiag(ui);
  return o;
}

/* ---------------------------------------------------------------- top level */
// The whole app, re-rendered from state on every change. `state.screen` selects the view;
// overlays stack on top. main.js sets #app.innerHTML to this and delegates clicks.
export function renderApp(state) {
  const ui = state;
  let page;
  switch (state.screen) {
    case "scanning":
      page = renderScanning(ui);
      break;
    case "empty":
      page = renderEmpty(ui);
      break;
    case "permission":
      page = renderPermission(ui);
      break;
    case "workspace":
      page = state.report
        ? renderWorkspace(state.report, state.index, ui)
        : renderEmpty(ui);
      break;
    default:
      page = renderWelcome(ui);
  }
  return `<div style="position:relative;min-height:100vh">${page}${renderOverlays(ui)}${renderToast(ui)}</div>`;
}
