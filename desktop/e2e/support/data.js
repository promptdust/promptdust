// Fixture data for the BDD suite. Shapes mirror the real backend contracts: `run_scan`
// returns an OutputDocument; `list_scans` returns index entries; `load_scan` returns a
// StoredRun ({ run_id, report, item_state }).

export const REPORT_WITH_DATA = {
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

export const REPORT_EMPTY = {
  generated_at: "2026-07-17T15:00:00Z",
  host: { os: "macos" },
  disk_encryption: "on",
  mode: "inventory",
  exposure: { score: 0, band: "minimal" },
  assurance: { score: 100, band: "high" },
  summary: { total_findings: 0, total_bytes: 0, by_exposure: {}, by_tool: {} },
  findings: [],
};

// Prior runs to seed history with (StoredRun shape). `unread` seeds the index-entry flag.
export const SEEDED_RUNS = [
  { run_id: "a".repeat(32), unread: false, report: REPORT_WITH_DATA, item_state: {} },
  {
    run_id: "b".repeat(32),
    unread: false,
    report: {
      ...REPORT_EMPTY,
      generated_at: "2026-07-10T09:03:00Z",
      exposure: { score: 44, band: "low" },
      assurance: { score: 76, band: "partial" },
      summary: { total_findings: 1, total_bytes: 46_871_347, by_exposure: { high: 1 }, by_tool: { Cursor: 1 } },
      findings: [REPORT_WITH_DATA.findings[2]],
    },
    item_state: {},
  },
];
