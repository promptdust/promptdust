//! Local scan-history persistence for the desktop **Inbox** (ADR-022).
//!
//! The desktop keeps a small, local, front-end-owned store of past scan runs so the Inbox
//! can show history and per-item read/pin/flag state. It writes **only** under the app's own
//! config dir (`promptdust_telemetry::config_dir()` → `scans/`), **never** a scanned file
//! (INV-1) and **never** a cloud-synced folder — the tool flags cloud-sync as an amplifier and
//! must not create one. `core` stays write-free; this is a front-end concern (INV-4 carve-out,
//! ADR-022).
//!
//! Layout under `<config>/promptdust/scans/`:
//! - `<run-id>.json` — the full report (`OutputDocument`, verbatim) plus per-finding state.
//! - `index.json` — the newest-first list of [`IndexEntry`] rows the Inbox rail renders
//!   without loading every report.
//!
//! `run-id` is a per-run random hex id (via [`promptdust_telemetry::run_id`]) — **not** a
//! persistent user/machine identifier (ADR-022). It is validated as strict lowercase hex
//! before it is ever used in a path, so a caller can never traverse out of `scans/`.
//!
//! A "Clear history" delete (the only delete ADR-022 blesses) is intentionally **not** here
//! yet: the GUI read-only audit forbids `remove_*` in the command layer, and teaching it the
//! config-dir carve-out is a deliberate maintainer edit — tracked as a follow-up. Until then
//! no run is ever removed (per-item state is mutated in place, but a run is never deleted).
//!
//! The commands do no locking; they assume the single-user desktop UI issues them serially
//! (as it does). A lost update under a hypothetical race orphans a run file (inert — see
//! `save_scan`), it does not corrupt shown data.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Subdirectory (under the config dir) that holds the history.
const SCANS_DIR: &str = "scans";
/// The newest-first index of runs.
const INDEX_FILE: &str = "index.json";

/// A score sub-object as it appears in the report (`{score, band}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub score: u32,
    pub band: String,
}

/// One Inbox-rail row: enough to render a run without loading its full report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    /// The run's random hex id (also its file name).
    pub run_id: String,
    /// ISO timestamp the scan ran (the report's `generated_at`).
    pub ran_at: String,
    /// Exposure `{score, band}` from the report (absent on an unscored report).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure: Option<Score>,
    /// Confidence `{score, band}` — the report's `assurance`, relabeled "Confidence" in the
    /// UI per ADR-018/redesign (the data field stays `assurance`; the Inbox owns this name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Score>,
    /// Headline for the rail: `"<size> · <top tool>"` (top tool by total bytes).
    pub headline: String,
    /// Number of findings in the run (`summary.total_findings`).
    pub trace_count: u64,
    /// `true` when written; cleared when the run is first opened.
    pub unread: bool,
}

/// Per-finding UI state, keyed by the finding's `path`. All default `false`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingState {
    pub read: bool,
    pub pinned: bool,
    pub flagged: bool,
}

/// A partial update to a finding's state — only the `Some` fields are applied.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FindingStatePatch {
    pub read: Option<bool>,
    pub pinned: Option<bool>,
    pub flagged: Option<bool>,
}

/// A stored run: the full report plus per-finding state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRun {
    pub run_id: String,
    /// The report (`OutputDocument`) exactly as produced by the scan.
    pub report: Value,
    /// Per-finding state, keyed by finding `path` (a map key only — never a filesystem path).
    #[serde(default)]
    pub item_state: BTreeMap<String, FindingState>,
}

/// The history directory, `<config_dir>/scans`.
fn scans_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(SCANS_DIR)
}

/// A run id is exactly 32 lowercase hex chars (128 bits, from [`promptdust_telemetry::run_id`]).
/// Validating before any path use makes traversal (`..`, `/`, absolute paths) impossible.
fn is_valid_run_id(run_id: &str) -> bool {
    run_id.len() == 32
        && run_id
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The on-disk path for a run, rejecting any id that isn't strict hex (traversal defense).
fn run_path(config_dir: &Path, run_id: &str) -> Result<PathBuf, String> {
    if !is_valid_run_id(run_id) {
        return Err(format!("invalid run id: {run_id}"));
    }
    Ok(scans_dir(config_dir).join(format!("{run_id}.json")))
}

/// Write `contents` to `path`. Plain write mirrors the consent store; a torn write only
/// affects our own local cache — a corrupt or lost index self-heals from the run files
/// (`rebuild_index`), which are written whole before the index is updated.
fn write_file(path: &Path, contents: &str) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|e| e.to_string())
}

/// Read + parse a run file at an already-validated path.
fn read_run(path: &Path) -> Result<StoredRun, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| format!("stored run is corrupt: {e}"))
}

/// Human-readable size, mirroring the UI's `humanSize` (and `cli::render::human_size`) so the
/// rail headline matches the rest of the app. Hoisting the two Rust copies into a shared
/// `core` helper is a possible follow-up — kept local here to keep this desktop change focused.
fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let units = ["KB", "MB", "GB", "TB"];
    let mut v = bytes as f64 / 1024.0;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", units[i])
}

/// The rail headline: `"<size> · <top tool>"`, the largest tool by summed `size_bytes`.
/// `"No traces"` when the run has no findings.
fn headline(report: &Value) -> String {
    let Some(findings) = report.get("findings").and_then(Value::as_array) else {
        return "No traces".to_string();
    };
    let mut by_tool: BTreeMap<&str, u64> = BTreeMap::new();
    for f in findings {
        let tool = f.get("tool").and_then(Value::as_str).unwrap_or("Unknown");
        let size = f.get("size_bytes").and_then(Value::as_u64).unwrap_or(0);
        *by_tool.entry(tool).or_default() += size;
    }
    match by_tool.into_iter().max_by_key(|&(_, bytes)| bytes) {
        Some((tool, bytes)) => format!("{} · {tool}", human_size(bytes)),
        None => "No traces".to_string(),
    }
}

/// Parse a `{score, band}` sub-object into a [`Score`] (`None` if absent/malformed).
fn parse_score(v: &Value) -> Option<Score> {
    let score = u32::try_from(v.get("score")?.as_u64()?).ok()?;
    let band = v.get("band")?.as_str()?.to_string();
    Some(Score { score, band })
}

/// Derive the Inbox-rail entry for a freshly-written run (always `unread`).
fn derive_entry(run_id: &str, report: &Value) -> IndexEntry {
    IndexEntry {
        run_id: run_id.to_string(),
        ran_at: report
            .get("generated_at")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        exposure: report.get("exposure").and_then(parse_score),
        // The report field is `assurance`; the Inbox surfaces it as "confidence".
        confidence: report.get("assurance").and_then(parse_score),
        headline: headline(report),
        trace_count: report
            .get("summary")
            .and_then(|s| s.get("total_findings"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        unread: true,
    }
}

/// Read `index.json`. If it's missing or won't parse, rebuild it from the run files on disk
/// (the run files are the source of truth; the index is a rebuildable cache) — so a torn or
/// hand-deleted index self-heals instead of bricking the Inbox. First run (no `scans/`) →
/// an empty list.
fn read_index(config_dir: &Path) -> Result<Vec<IndexEntry>, String> {
    let path = scans_dir(config_dir).join(INDEX_FILE);
    match std::fs::read_to_string(&path) {
        Ok(s) => match serde_json::from_str(&s) {
            Ok(index) => Ok(index),
            Err(_) => rebuild_index(config_dir),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => rebuild_index(config_dir),
        Err(e) => Err(e.to_string()),
    }
}

/// Reconstruct the index by re-deriving an entry from every stored run file (newest first),
/// then persist the healed index. Individually-corrupt run files are skipped, not fatal.
/// `unread` resets to `true` for recovered runs (it lives only in the index, not the run
/// file) — an acceptable cost of recovering from a lost index vs. losing all history.
fn rebuild_index(config_dir: &Path) -> Result<Vec<IndexEntry>, String> {
    let read_dir = match std::fs::read_dir(scans_dir(config_dir)) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let mut entries: Vec<IndexEntry> = Vec::new();
    for ent in read_dir {
        let path = ent.map_err(|e| e.to_string())?.path();
        // Only `<run-id>.json` run files — skip index.json and any non-hex stem.
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !is_valid_run_id(stem) {
            continue;
        }
        let Ok(s) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(run) = serde_json::from_str::<StoredRun>(&s) else {
            continue;
        };
        // Key off the validated *filename*, not the file's inner `run_id` (which could
        // disagree), so the healed index can never point at a run file that isn't there.
        entries.push(derive_entry(stem, &run.report));
    }
    // Best-effort newest-first by the report timestamp. RFC3339 UTC strings sort
    // lexicographically ~chronologically; ties within a second (or a missing timestamp) are
    // arbitrary. The live index uses insertion order, which agrees because each real save is a
    // fresh "now" scan.
    entries.sort_by(|a, b| b.ran_at.cmp(&a.ran_at));
    if !entries.is_empty() {
        write_index(config_dir, &entries)?;
    }
    Ok(entries)
}

/// Write `index.json`, creating `scans/` if needed. Plain (non-atomic) write — see
/// `write_file`; a torn index self-heals via `rebuild_index`.
fn write_index(config_dir: &Path, index: &[IndexEntry]) -> Result<(), String> {
    let dir = scans_dir(config_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(index).map_err(|e| e.to_string())?;
    write_file(&dir.join(INDEX_FILE), &json)
}

/// Persist a scan `report` (its JSON) and prepend it to the index. Returns the new entry.
/// The run file is written **before** the index is updated, so the index never references a
/// missing run.
pub fn save_scan(config_dir: &Path, report_json: &str) -> Result<IndexEntry, String> {
    let report: Value =
        serde_json::from_str(report_json).map_err(|e| format!("invalid report json: {e}"))?;
    if !report.is_object() {
        return Err("report is not a JSON object".to_string());
    }
    // A fresh per-run random hex id, used as the file name (validated as strict hex so it can
    // never traverse). Refuse to overwrite an existing file: that turns any collision into an
    // error instead of data loss, and keeps the entropy-failure fallback (a constant id, see
    // `promptdust_telemetry::run_id`) from silently clobbering a prior run.
    let run_id = promptdust_telemetry::run_id();
    let path = run_path(config_dir, &run_id)?;
    if path.exists() {
        return Err("scan-history run-id collision — please retry".to_string());
    }
    let entry = derive_entry(&run_id, &report);

    // Read the current index BEFORE writing the run file: if the index is missing it rebuilds
    // from the runs that already exist, and we must not let it see the run we're about to add
    // (that would double-count it). The run file is written before the index is persisted, so
    // the index never points at a missing run; the reverse — a crash (or a concurrent racing
    // command, since these are unlocked) between the two writes — can leave an orphan run file
    // that a *valid* index doesn't list. That orphan is inert (never shown), and is reclaimed
    // by the next rebuild or a future "clear history"; it is not shown as wrong data.
    let mut index = read_index(config_dir)?;
    let run = StoredRun {
        run_id,
        report,
        item_state: BTreeMap::new(),
    };
    std::fs::create_dir_all(scans_dir(config_dir)).map_err(|e| e.to_string())?;
    write_file(
        &path,
        &serde_json::to_string_pretty(&run).map_err(|e| e.to_string())?,
    )?;
    index.insert(0, entry.clone());
    write_index(config_dir, &index)?;
    Ok(entry)
}

/// The newest-first list of runs (empty on first launch).
pub fn list_scans(config_dir: &Path) -> Result<Vec<IndexEntry>, String> {
    read_index(config_dir)
}

/// Load a stored run (full report + per-finding state) by id.
pub fn load_scan(config_dir: &Path, run_id: &str) -> Result<StoredRun, String> {
    read_run(&run_path(config_dir, run_id)?)
}

/// Clear a run's unread flag (when the user opens it). Errors if the run isn't in the index.
pub fn mark_scan_read(config_dir: &Path, run_id: &str) -> Result<(), String> {
    if !is_valid_run_id(run_id) {
        return Err(format!("invalid run id: {run_id}"));
    }
    let mut index = read_index(config_dir)?;
    let entry = index
        .iter_mut()
        .find(|e| e.run_id == run_id)
        .ok_or_else(|| format!("no such run: {run_id}"))?;
    if !entry.unread {
        return Ok(()); // already read — nothing to write
    }
    entry.unread = false;
    write_index(config_dir, &index)
}

/// Apply a partial state update to one finding in a run (keyed by the finding's `path`).
/// `path` is used only as a map key — never touched as a filesystem path.
pub fn set_finding_state(
    config_dir: &Path,
    run_id: &str,
    path: &str,
    patch: &FindingStatePatch,
) -> Result<(), String> {
    let run_file = run_path(config_dir, run_id)?;
    let mut run = read_run(&run_file)?;
    let st = run.item_state.entry(path.to_string()).or_default();
    if let Some(read) = patch.read {
        st.read = read;
    }
    if let Some(pinned) = patch.pinned {
        st.pinned = pinned;
    }
    if let Some(flagged) = patch.flagged {
        st.flagged = flagged;
    }
    write_file(
        &run_file,
        &serde_json::to_string_pretty(&run).map_err(|e| e.to_string())?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal report shaped like the fields `derive_entry` reads (extra fields ignored).
    fn sample_report(generated_at: &str, exposure: u32, assurance: u32) -> String {
        serde_json::json!({
            "generated_at": generated_at,
            "exposure": { "score": exposure, "band": "high" },
            "assurance": { "score": assurance, "band": "partial" },
            "summary": { "total_findings": 3 },
            "findings": [
                { "tool": "Claude Code", "path": "/Users/x/.claude/projects/a", "size_bytes": 2000 },
                { "tool": "Claude Code", "path": "/Users/x/.claude/history.jsonl", "size_bytes": 1_000_000 },
                { "tool": "Cursor", "path": "/Users/x/Cursor/state.vscdb", "size_bytes": 500 }
            ]
        })
        .to_string()
    }

    #[test]
    fn save_then_list_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let entry = save_scan(dir.path(), &sample_report("2026-07-17T10:00:00Z", 71, 88)).unwrap();
        // Derived entry maps assurance→confidence and reads the summary/timestamp.
        assert_eq!(entry.ran_at, "2026-07-17T10:00:00Z");
        assert_eq!(entry.exposure.as_ref().unwrap().score, 71);
        assert_eq!(entry.confidence.as_ref().unwrap().score, 88);
        assert_eq!(entry.trace_count, 3);
        assert!(entry.unread);
        // Top tool by bytes is Claude Code (1.0MB + 2KB > Cursor 500B).
        assert!(
            entry.headline.ends_with("· Claude Code"),
            "{}",
            entry.headline
        );

        let listed = list_scans(dir.path()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0], entry);

        let run = load_scan(dir.path(), &entry.run_id).unwrap();
        assert_eq!(run.run_id, entry.run_id);
        assert_eq!(run.report["exposure"]["score"], 71);
        assert!(run.item_state.is_empty());
    }

    #[test]
    fn save_prepends_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let first = save_scan(dir.path(), &sample_report("2026-07-16T09:00:00Z", 40, 70)).unwrap();
        let second = save_scan(dir.path(), &sample_report("2026-07-17T09:00:00Z", 60, 80)).unwrap();
        let listed = list_scans(dir.path()).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].run_id, second.run_id, "newest first");
        assert_eq!(listed[1].run_id, first.run_id);
        assert_ne!(first.run_id, second.run_id, "each run gets a fresh id");
    }

    #[test]
    fn list_is_empty_before_any_scan() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_scans(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn mark_scan_read_clears_unread_and_errors_on_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let e = save_scan(dir.path(), &sample_report("2026-07-17T10:00:00Z", 71, 88)).unwrap();
        assert!(list_scans(dir.path()).unwrap()[0].unread);
        mark_scan_read(dir.path(), &e.run_id).unwrap();
        assert!(!list_scans(dir.path()).unwrap()[0].unread, "unread cleared");
        // Marking an already-read run again is an idempotent no-op, still Ok.
        mark_scan_read(dir.path(), &e.run_id).unwrap();
        assert!(!list_scans(dir.path()).unwrap()[0].unread);
        // A valid-format but absent id is a defined error, not a silent success.
        assert!(mark_scan_read(dir.path(), &"a".repeat(32)).is_err());
    }

    #[test]
    fn set_finding_state_applies_partial_patch_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let e = save_scan(dir.path(), &sample_report("2026-07-17T10:00:00Z", 71, 88)).unwrap();
        let p = "/Users/x/.claude/history.jsonl";
        set_finding_state(
            dir.path(),
            &e.run_id,
            p,
            &FindingStatePatch {
                pinned: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        // A second partial patch must not clobber the first field.
        set_finding_state(
            dir.path(),
            &e.run_id,
            p,
            &FindingStatePatch {
                read: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let st = load_scan(dir.path(), &e.run_id)
            .unwrap()
            .item_state
            .remove(p)
            .unwrap();
        assert_eq!(
            st,
            FindingState {
                read: true,
                pinned: true,
                flagged: false
            }
        );
    }

    #[test]
    fn invalid_run_id_is_rejected_before_any_path_use() {
        let dir = tempfile::tempdir().unwrap();
        // Traversal / non-hex ids never reach the filesystem.
        for bad in [
            "../../../../etc/passwd",
            "..",
            "abc", // too short
            "/absolute/path",
            "ABCDEF0123456789ABCDEF0123456789", // uppercase
            &"a".repeat(31),                    // 31 chars
            &format!("{}/", "a".repeat(31)),    // contains a separator
        ] {
            assert!(!is_valid_run_id(bad), "{bad} should be invalid");
            assert!(load_scan(dir.path(), bad).is_err(), "load rejects {bad}");
            assert!(
                mark_scan_read(dir.path(), bad).is_err(),
                "mark rejects {bad}"
            );
            assert!(
                set_finding_state(dir.path(), bad, "p", &FindingStatePatch::default()).is_err(),
                "set rejects {bad}"
            );
        }
        // A well-formed id is accepted by the validator.
        assert!(is_valid_run_id(&"a".repeat(32)));
    }

    #[test]
    fn corrupt_index_self_heals_from_run_files() {
        let dir = tempfile::tempdir().unwrap();
        let a = save_scan(dir.path(), &sample_report("2026-07-16T09:00:00Z", 40, 70)).unwrap();
        let b = save_scan(dir.path(), &sample_report("2026-07-17T09:00:00Z", 60, 80)).unwrap();
        // Corrupt the index; the run files are intact, so list rebuilds from them, newest-first.
        std::fs::write(scans_dir(dir.path()).join(INDEX_FILE), "{ not json").unwrap();
        let ids: Vec<_> = list_scans(dir.path())
            .unwrap()
            .into_iter()
            .map(|e| e.run_id)
            .collect();
        assert_eq!(ids, [b.run_id, a.run_id], "rebuilt newest-first");
    }

    #[test]
    fn save_scan_rejects_non_object_reports() {
        let dir = tempfile::tempdir().unwrap();
        // Non-JSON and valid-JSON-but-not-an-object are both rejected (a report is an object),
        // so a scalar can't slip through and persist a junk row.
        for bad in ["{ not json", "null", "5", "true", "\"a string\"", "[1, 2]"] {
            assert!(save_scan(dir.path(), bad).is_err(), "should reject {bad}");
        }
        // A rejected save persists nothing.
        assert!(list_scans(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn load_scan_surfaces_a_corrupt_run_file() {
        let dir = tempfile::tempdir().unwrap();
        let e = save_scan(dir.path(), &sample_report("2026-07-17T10:00:00Z", 71, 88)).unwrap();
        // Overwrite the (valid-hex) run file with garbage → a defined "corrupt" error.
        let run_file = scans_dir(dir.path()).join(format!("{}.json", e.run_id));
        std::fs::write(&run_file, "{ not json").unwrap();
        let err = load_scan(dir.path(), &e.run_id).unwrap_err();
        assert!(err.contains("corrupt"), "{err}");
    }

    #[test]
    fn set_finding_state_on_absent_run_errors_without_creating_it() {
        let dir = tempfile::tempdir().unwrap();
        let absent = "a".repeat(32); // valid hex, but no such run file
        assert!(set_finding_state(
            dir.path(),
            &absent,
            "/some/path",
            &FindingStatePatch {
                pinned: Some(true),
                ..Default::default()
            },
        )
        .is_err());
        // It did not silently create a run for the absent id.
        assert!(load_scan(dir.path(), &absent).is_err());
    }

    #[test]
    fn save_writes_only_under_the_scans_dir() {
        // INV-1/INV-4: the store never writes outside <config_dir>/scans.
        let root = tempfile::tempdir().unwrap();
        let config_dir = root.path().join("cfg");
        save_scan(&config_dir, &sample_report("2026-07-17T10:00:00Z", 71, 88)).unwrap();
        // The only thing created under the config dir is the scans/ subtree.
        let entries: Vec<_> = std::fs::read_dir(&config_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, [std::ffi::OsString::from(SCANS_DIR)]);
    }

    #[test]
    fn human_size_and_headline_math() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1_048_576), "1.0 MB");
        // Empty findings → "No traces"; otherwise top tool by bytes.
        assert_eq!(
            headline(&serde_json::json!({ "findings": [] })),
            "No traces"
        );
        assert_eq!(headline(&serde_json::json!({})), "No traces");
    }
}
