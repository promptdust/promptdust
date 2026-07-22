//! Tauri command layer for the promptdust desktop app.
//!
//! It exposes read-only / user-initiated capabilities only:
//! - `run_scan` — run the core scan and return the JSON report (off the UI thread).
//! - `diagnostics` — return the redacted, path-free diagnostics bundle for a bug report.
//! - `telemetry_status` / `telemetry_set_enabled` — read/set the opt-in telemetry choice
//!   (writes only the consent file in the app's own config dir).
//! - `telemetry_preview` — return the exact anonymous payload telemetry would send.
//! - `reveal` — reveal an artifact's *location* in the OS file manager (never opens
//!   file content).
//! - `export_report` — write a report the user explicitly asked to export.
//! - `save_scan` / `list_scans` / `load_scan` / `mark_scan_read` / `set_finding_state` —
//!   the local **Inbox** store (ADR-022): persist past runs + per-item read/pin/flag state
//!   under the app's own config dir. Writes never touch a scanned file; the store never
//!   removes a run (a user-facing "Clear history" delete is a deferred follow-up).
//! - `share` — hand a caller-composed *metadata* summary to the native macOS Share sheet
//!   (`NSSharingServicePicker`); the OS resolves the installed targets. macOS only.
//!
//! There is deliberately **no** command that modifies or deletes a scanned file.

mod history;
mod share;

use std::path::{Path, PathBuf};

use promptdust_core::{
    scan, DiagnosticsDocument, Host, Mode, OutputDocument, ScanConfig, ScanResult,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Run a scan and return the JSON output document. Pure enough to unit-test.
fn scan_to_json(no_slow: bool, mode: Mode) -> Result<String, String> {
    let mut cfg =
        ScanConfig::detect().ok_or_else(|| "could not determine home directory".to_string())?;
    cfg.no_slow = no_slow;
    // Supply "now" so the clockless core can score recency + the dual number.
    cfg.now_epoch = Some(OffsetDateTime::now_utc().unix_timestamp());
    cfg.mode = mode;
    let result = scan(&cfg);
    Ok(build_json(&result))
}

/// Resolve the consent ring the UI requested into a scan `Mode`. Only Ring 0 (Inventory) has
/// collectors today; deeper rings the UI may name — e.g. `"usage"` (running processes / ports
/// / persistence) — are a separate explicit opt-in whose collectors are not built yet,
/// so every request resolves to Inventory. This is the consent seam: add arms here as rings
/// ship (their matching UI toggle stays disabled until then).
fn parse_mode(_mode: Option<&str>) -> Mode {
    Mode::Inventory
}

fn build_json(result: &ScanResult) -> String {
    OutputDocument::new(result, now_rfc3339(), host()).to_json_pretty()
}

/// Build the redacted diagnostics bundle: a scan projected to counts/versions only — no
/// path, no content — for the user to inspect and paste into a bug report.
fn diagnostics_to_json(no_slow: bool) -> Result<String, String> {
    let mut cfg =
        ScanConfig::detect().ok_or_else(|| "could not determine home directory".to_string())?;
    cfg.no_slow = no_slow;
    // No `now_epoch`: the bundle is count-only, so skip the dual-score pass.
    let started = std::time::Instant::now();
    let result = scan(&cfg);
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok(DiagnosticsDocument::new(
        &result,
        now_rfc3339(),
        env!("CARGO_PKG_VERSION").to_string(),
        host(),
        Some(elapsed_ms),
    )
    .to_json_pretty())
}

fn host() -> Host {
    Host {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        os_version: os_version(),
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
}

fn os_version() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .filter(|v| !v.is_empty())
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Reveal an artifact's containing location in the OS file manager. This opens the
/// *folder*, never the file's contents.
fn reveal_in_os(path: &Path) -> Result<(), String> {
    let spawn = |mut c: std::process::Command| c.spawn().map(|_| ()).map_err(|e| e.to_string());
    #[cfg(target_os = "macos")]
    {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(path);
        spawn(c)
    }
    #[cfg(target_os = "windows")]
    {
        let mut c = std::process::Command::new("explorer");
        c.arg("/select,").arg(path);
        spawn(c)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = path.parent().unwrap_or(path);
        let mut c = std::process::Command::new("xdg-open");
        c.arg(dir);
        spawn(c)
    }
}

/// Write a report the user explicitly chose to export. This writes only to the
/// export destination, never a scanned file.
fn write_report(dest: &Path, contents: &str) -> Result<(), String> {
    std::fs::write(dest, contents).map_err(|e| e.to_string())
}

/// A destination for an exported report: `~/Downloads/promptdust-report-<ts>.<ext>`
/// (falling back to the home directory). `extension` is normalized to `md` or `json`.
fn export_destination(extension: &str) -> Result<PathBuf, String> {
    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| "could not determine a download directory".to_string())?;
    let ts = OffsetDateTime::now_utc().unix_timestamp();
    let ext = match extension {
        "md" | "markdown" => "md",
        _ => "json",
    };
    Ok(dir.join(format!("promptdust-report-{ts}.{ext}")))
}

#[tauri::command]
async fn run_scan(no_slow: bool, mode: Option<String>) -> Result<String, String> {
    let mode = parse_mode(mode.as_deref());
    tauri::async_runtime::spawn_blocking(move || scan_to_json(no_slow, mode))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn diagnostics(no_slow: bool) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || diagnostics_to_json(no_slow))
        .await
        .map_err(|e| e.to_string())?
}

/// The telemetry consent + env state, as JSON, for the UI to show.
fn telemetry_status_json(config_dir: &Path) -> String {
    let consent = promptdust_telemetry::Consent::load(config_dir);
    serde_json::json!({
        "enabled": consent.is_enabled(),
        "suppressed_by_env": promptdust_telemetry::suppressed_by_env(),
    })
    .to_string()
}

/// Build the anonymous telemetry payload for a fresh scan (what `telemetry_preview` returns).
fn telemetry_preview_json(no_slow: bool) -> Result<String, String> {
    let mut cfg =
        ScanConfig::detect().ok_or_else(|| "could not determine home directory".to_string())?;
    cfg.no_slow = no_slow;
    let started = std::time::Instant::now();
    let result = scan(&cfg);
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let flags = if no_slow {
        vec!["no_slow".to_string()]
    } else {
        Vec::new()
    };
    Ok(promptdust_telemetry::Payload::new(
        &result,
        env!("CARGO_PKG_VERSION").to_string(),
        std::env::consts::OS.to_string(),
        std::env::consts::ARCH.to_string(),
        Some(elapsed_ms),
        flags,
    )
    .to_json_pretty())
}

#[tauri::command]
fn telemetry_status() -> Result<String, String> {
    Ok(telemetry_status_json(&config_dir()?))
}

#[tauri::command]
fn telemetry_set_enabled(enabled: bool) -> Result<(), String> {
    let dir = config_dir()?;
    let mut consent = promptdust_telemetry::Consent::load(&dir);
    consent.state = if enabled {
        promptdust_telemetry::TelemetryState::Enabled
    } else {
        promptdust_telemetry::TelemetryState::Disabled
    };
    consent.notified = true;
    consent.save(&dir).map_err(|e| e.to_string())
}

#[tauri::command]
async fn telemetry_preview(no_slow: bool) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || telemetry_preview_json(no_slow))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn reveal(path: String) -> Result<(), String> {
    reveal_in_os(Path::new(&path))
}

#[tauri::command]
fn export_report(contents: String, extension: String) -> Result<String, String> {
    let dest = export_destination(&extension)?;
    write_report(&dest, &contents)?;
    Ok(dest.to_string_lossy().into_owned())
}

/// The app's config dir, or a user-facing error if it can't be determined.
fn config_dir() -> Result<PathBuf, String> {
    promptdust_telemetry::config_dir()
        .ok_or_else(|| "could not determine a config directory".to_string())
}

/// Persist a just-run report to the local Inbox store; returns the new index entry (JSON).
#[tauri::command]
fn save_scan(report_json: String) -> Result<String, String> {
    let entry = history::save_scan(&config_dir()?, &report_json)?;
    serde_json::to_string(&entry).map_err(|e| e.to_string())
}

/// The newest-first list of stored runs (JSON array of index entries).
#[tauri::command]
fn list_scans() -> Result<String, String> {
    let index = history::list_scans(&config_dir()?)?;
    serde_json::to_string(&index).map_err(|e| e.to_string())
}

/// Load a stored run (full report + per-finding state) by id (JSON).
#[tauri::command]
fn load_scan(run_id: String) -> Result<String, String> {
    let run = history::load_scan(&config_dir()?, &run_id)?;
    serde_json::to_string(&run).map_err(|e| e.to_string())
}

/// Clear a run's unread flag (called when the user opens it).
#[tauri::command]
fn mark_scan_read(run_id: String) -> Result<(), String> {
    history::mark_scan_read(&config_dir()?, &run_id)
}

/// Apply a partial read/pin/flag update to one finding in a run.
#[tauri::command]
fn set_finding_state(
    run_id: String,
    path: String,
    patch: history::FindingStatePatch,
) -> Result<(), String> {
    history::set_finding_state(&config_dir()?, &run_id, &path, &patch)
}

/// User-facing guidance printed when a release build panics (consent-based crash reporting).
/// human-panic writes a redacted report to a temp file and prints this; nothing is ever sent
/// automatically.
const CRASH_SUPPORT: &str = "Please open an issue at \
    https://github.com/promptdust/promptdust/issues and attach the report file named \
    above. It contains a technical crash backtrace, your OS, and the app version — never \
    your scanned files, their paths, or any conversation content.";

/// Crash-reporter metadata for `human_panic::setup_panic!`.
fn crash_metadata() -> human_panic::Metadata {
    human_panic::Metadata::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
        .homepage("https://github.com/promptdust/promptdust")
        .support(CRASH_SUPPORT)
}

/// An env value "disables" the crash reporter when it is present and not empty/`0`
/// (the `DO_NOT_TRACK` convention).
fn env_disables(val: Option<std::ffi::OsString>) -> bool {
    val.and_then(|v| v.into_string().ok())
        .is_some_and(|v| !v.is_empty() && v != "0")
}

/// The local crash report is written by default, but suppressed by `DO_NOT_TRACK`, the
/// `PROMPTDUST_NO_CRASH_REPORT` kill-switch, or CI. Nothing is ever *sent* without the
/// user's explicit action, regardless.
fn crash_reporting_enabled() -> bool {
    !env_disables(std::env::var_os("DO_NOT_TRACK"))
        && !env_disables(std::env::var_os("PROMPTDUST_NO_CRASH_REPORT"))
        && !env_disables(std::env::var_os("CI"))
}

/// Launch the desktop application.
pub fn run() {
    // Crash reporting: on a *release* panic, write a redacted report to a temp file and tell
    // the user how to share it — opt-out via DO_NOT_TRACK / the kill-switch / CI. No-op in
    // debug; never auto-sends.
    if crash_reporting_enabled() {
        human_panic::setup_panic!(crash_metadata());
    }
    tauri::Builder::default()
        // Opt-in, signed self-update (Q-03): the plugin never checks on its own — a
        // check only happens when the user asks (the "Check for updates" action in the
        // UI). Downloaded updates are verified against the pubkey in tauri.conf.json
        // before install. This registers the plugin's commands, not one of ours; the
        // four read-only app commands below are unchanged.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            run_scan,
            diagnostics,
            telemetry_status,
            telemetry_set_enabled,
            telemetry_preview,
            reveal,
            export_report,
            save_scan,
            list_scans,
            load_scan,
            mark_scan_read,
            set_finding_state,
            share::share
        ])
        .run(tauri::generate_context!())
        .expect("error while running the promptdust application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_to_json_produces_valid_document() {
        let json = scan_to_json(true, Mode::Inventory).expect("scan should succeed");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["schema_version"], promptdust_core::SCHEMA_VERSION);
        assert!(v["generated_at"].as_str().unwrap().contains('T'));
        assert!(v["findings"].is_array());
        // Ring 0 is the only mode collected today.
        assert_eq!(v["mode"], "inventory");
    }

    #[test]
    fn history_entry_matches_the_real_scan_contract() {
        // Feed a REAL OutputDocument (not a hand-built fixture) through the history store, so a
        // rename of exposure/assurance/summary in core's contract fails here instead of
        // silently zeroing the Inbox rail. The scan is always scored (now_epoch supplied), so
        // both scores are present in the document.
        let json = scan_to_json(true, Mode::Inventory).expect("scan should succeed");
        let report: serde_json::Value = serde_json::from_str(&json).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let entry = history::save_scan(dir.path(), &json).expect("save should succeed");
        assert!(
            entry.exposure.is_some(),
            "exposure key path drifted from the contract"
        );
        assert!(
            entry.confidence.is_some(),
            "assurance→confidence key path drifted from the contract"
        );
        assert!(
            entry.ran_at.contains('T'),
            "ran_at (from generated_at) drifted: {}",
            entry.ran_at
        );
        assert_eq!(
            entry.trace_count,
            report["summary"]["total_findings"].as_u64().unwrap(),
            "trace_count (summary.total_findings) drifted"
        );
    }

    #[test]
    fn diagnostics_to_json_is_redacted_and_versioned() {
        let json = diagnostics_to_json(true).expect("diagnostics should succeed");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["kind"], "promptdust-diagnostics");
        assert_eq!(v["tool_version"], env!("CARGO_PKG_VERSION"));
        assert!(v["summary"]["by_definition"].is_object());
        // The bundle never carries raw findings (which would hold absolute paths).
        assert!(v.get("findings").is_none());
    }

    #[test]
    fn crash_support_message_is_consent_based() {
        // The message must guide *manual* sharing, disclaim content, and never imply auto-send.
        assert!(CRASH_SUPPORT.contains("open an issue"));
        let lower = CRASH_SUPPORT.to_lowercase();
        assert!(lower.contains("conversation content") && lower.contains("scanned files"));
        assert!(!lower.contains("automatically"));
    }

    #[test]
    fn crash_reporting_respects_do_not_track_and_kill_switch() {
        // Pure gate logic: the local report is on by default, but DO_NOT_TRACK / the
        // kill-switch / CI each suppress it, per DO_NOT_TRACK semantics (present + not
        // empty/`0`).
        assert!(!env_disables(None), "unset → enabled");
        assert!(!env_disables(Some("".into())), "empty → enabled");
        assert!(!env_disables(Some("0".into())), "\"0\" → enabled");
        assert!(env_disables(Some("1".into())), "set → disabled");
        assert!(env_disables(Some("true".into())), "truthy → disabled");
        let _ = crash_metadata();
    }

    #[test]
    fn telemetry_status_reflects_the_consent_store() {
        let dir = tempfile::tempdir().unwrap();
        // Default: disabled.
        let s: serde_json::Value =
            serde_json::from_str(&telemetry_status_json(dir.path())).unwrap();
        assert_eq!(s["enabled"], false);
        // Enabling via the consent store is reflected by the status projection.
        promptdust_telemetry::Consent {
            state: promptdust_telemetry::TelemetryState::Enabled,
            notified: true,
        }
        .save(dir.path())
        .unwrap();
        let s2: serde_json::Value =
            serde_json::from_str(&telemetry_status_json(dir.path())).unwrap();
        assert_eq!(s2["enabled"], true);
    }

    #[test]
    fn telemetry_preview_json_is_the_anonymous_payload() {
        let json = telemetry_preview_json(true).expect("preview should succeed");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["kind"], "promptdust-telemetry");
        assert_eq!(v["run_id"].as_str().unwrap().len(), 32);
        assert!(
            v.get("findings").is_none(),
            "the payload never carries raw findings"
        );
    }

    #[test]
    fn parse_mode_gates_deeper_rings_to_inventory() {
        // The consent seam: every requested ring resolves to Ring 0 today because the deeper
        // collectors are not built yet. Adding a real ring must update this deliberately.
        for requested in [None, Some("inventory"), Some("usage"), Some("nonsense")] {
            assert_eq!(parse_mode(requested), Mode::Inventory, "{requested:?}");
        }
    }

    #[test]
    fn write_report_writes_the_chosen_file() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.json");
        write_report(&dest, "{\"ok\":true}").unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "{\"ok\":true}");
    }

    #[test]
    fn export_destination_honors_extension() {
        let j = export_destination("json").expect("a download/home dir");
        assert_eq!(j.extension().and_then(|e| e.to_str()), Some("json"));
        let m = export_destination("md").expect("a download/home dir");
        assert_eq!(m.extension().and_then(|e| e.to_str()), Some("md"));
        // Unknown formats fall back to json, never an arbitrary extension.
        let f = export_destination("exe").expect("a download/home dir");
        assert_eq!(f.extension().and_then(|e| e.to_str()), Some("json"));
        assert!(m
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("promptdust-report-"));
    }
}
