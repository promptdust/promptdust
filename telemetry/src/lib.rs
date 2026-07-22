//! Opt-in, anonymous usage telemetry for the promptdust **front-ends** (`cli` / `desktop`).
//!
//! This crate lives deliberately outside `promptdust-core`: the engine must carry no
//! telemetry (INV-5). It holds the consent store, the enable/disable gate, the anonymous
//! [`Payload`] (built from core's path-free [`RedactedSummary`]), and a pluggable [`Sender`]
//! whose only implementation today is a no-op — there is **no live backend and no networking
//! dependency yet** (the epic ships the client stubbed).
//!
//! Consent is **opt-in**: telemetry is off unless the user explicitly enables it, and even
//! then it is suppressed by `DO_NOT_TRACK`, a `PROMPTDUST_TELEMETRY` kill-switch, or CI. The
//! payload carries only counts/versions + a **per-run random id** (regenerated every run,
//! never persisted) — no path, no content, no stable identifier.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use promptdust_core::{RedactedSummary, ScanResult};

/// The file (under the app's config dir) that records the user's telemetry choice.
const CONSENT_FILE: &str = "consent.json";

/// Whether the user has opted into telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TelemetryState {
    /// Off — the default until the user explicitly opts in.
    #[default]
    Disabled,
    /// On — the user ran `telemetry enable`.
    Enabled,
}

/// The persisted consent record (`<config>/promptdust/consent.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Consent {
    /// The user's choice (default `Disabled`).
    #[serde(default)]
    pub state: TelemetryState,
    /// Whether the one-time first-run notice has been shown (so it never repeats).
    #[serde(default)]
    pub notified: bool,
}

impl Consent {
    /// Load consent from `<config_dir>/consent.json`. A missing or malformed file yields the
    /// default (telemetry **off**, not yet notified) — never an error, never enabled.
    #[must_use]
    pub fn load(config_dir: &Path) -> Self {
        std::fs::read_to_string(config_dir.join(CONSENT_FILE))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Persist consent to `<config_dir>/consent.json`, creating the dir if needed. This is
    /// the only file the telemetry client writes, and it lands in the app's own config dir
    /// (the INV-4 carve-out) — never a scanned path.
    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let json = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string());
        std::fs::write(config_dir.join(CONSENT_FILE), json)
    }

    /// `true` when the user has opted in.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.state == TelemetryState::Enabled
    }
}

/// The app's config dir (`<config>/promptdust`), honoring the `PROMPTDUST_CONFIG_DIR` test
/// override. `None` if no config dir can be determined.
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("PROMPTDUST_CONFIG_DIR") {
        if !d.is_empty() {
            return Some(PathBuf::from(d));
        }
    }
    dirs::config_dir().map(|c| c.join("promptdust"))
}

/// A value that is set and not empty/`0` (the `DO_NOT_TRACK` / CI convention).
fn set_nonzero(val: Option<&str>) -> bool {
    val.is_some_and(|v| !v.is_empty() && v != "0")
}

/// A value that is explicitly falsy (`0`/`false`/`no`/`off`) — the kill-switch convention.
fn falsy(val: Option<&str>) -> bool {
    val.map(|v| v.trim().to_ascii_lowercase())
        .is_some_and(|v| matches!(v.as_str(), "0" | "false" | "no" | "off"))
}

/// `true` when the environment forces telemetry **off** regardless of stored consent:
/// `DO_NOT_TRACK` (set, non-`0`), the `PROMPTDUST_TELEMETRY` kill-switch set falsy, or CI.
/// Respected even before the first-run notice (for CI / enterprise).
#[must_use]
pub fn suppressed_by_env() -> bool {
    let get = |name: &str| std::env::var(name).ok();
    set_nonzero(get("DO_NOT_TRACK").as_deref())
        || falsy(get("PROMPTDUST_TELEMETRY").as_deref())
        || set_nonzero(get("CI").as_deref())
}

/// Whether telemetry should actually be collected: the user opted in **and** the environment
/// does not suppress it.
#[must_use]
pub fn is_active(consent: &Consent) -> bool {
    consent.is_enabled() && !suppressed_by_env()
}

/// A fresh, unlinkable per-run identifier (128 random bits, hex). Never persisted — it lets a
/// backend de-duplicate events *within one run* without linking runs to each other or to a
/// machine. Falls back to a constant (never panics) if OS entropy is unavailable.
#[must_use]
pub fn run_id() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::getrandom(&mut bytes).is_err() {
        return "0".repeat(32);
    }
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(32), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The anonymous telemetry payload — exactly what a [`Sender`] would transmit (and what
/// `telemetry preview` prints). Built from core's path-free [`RedactedSummary`] plus a
/// per-run random id, the OS/arch, tool version, scan duration, and which feature flags were
/// used. **No path, no content, no persistent id, no `os_version`** (kept coarse on purpose).
#[derive(Debug, Serialize)]
pub struct Payload {
    /// Stable marker identifying the payload kind.
    kind: &'static str,
    /// The `promptdust` version that produced it.
    tool_version: String,
    /// Fresh per-run random id (see [`run_id`]).
    run_id: String,
    /// Operating system (`macos`/`linux`/`windows`), coarse.
    os: String,
    /// CPU architecture, coarse.
    arch: String,
    /// Wall-clock scan duration, if measured.
    #[serde(skip_serializing_if = "Option::is_none")]
    scan_duration_ms: Option<u64>,
    /// Which optional features/flags were active this run (names only).
    feature_flags: Vec<String>,
    /// The path-scrubbed, count-only projection of the scan (core, canary-guarded).
    summary: RedactedSummary,
}

impl Payload {
    /// Assemble the payload from a scan + caller-supplied host/version/duration/flags. The
    /// scan is projected through [`RedactedSummary`], so no path or content can appear; the
    /// `run_id` is freshly random every call.
    #[must_use]
    pub fn new(
        result: &ScanResult,
        tool_version: String,
        os: String,
        arch: String,
        scan_duration_ms: Option<u64>,
        feature_flags: Vec<String>,
    ) -> Self {
        Self {
            kind: "promptdust-telemetry",
            tool_version,
            run_id: run_id(),
            os,
            arch,
            scan_duration_ms,
            feature_flags,
            summary: RedactedSummary::from_scan(result),
        }
    }

    /// The exact bytes a sender would transmit (also what `telemetry preview` prints).
    #[must_use]
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// A destination for a telemetry payload. The only implementation today is [`NoopSender`] —
/// there is no live backend yet (the epic ships the client stubbed), hence **no networking
/// dependency**. A real HTTP sender lands with the backend, in a front-end crate, never core.
pub trait Sender {
    /// Transmit the serialized payload. Returns an error string on failure; must never panic.
    fn send(&self, payload_json: &str) -> Result<(), String>;
}

/// The default sender: does nothing (no backend). Keeps the client fully wired and testable
/// with zero network surface.
#[derive(Debug, Default)]
pub struct NoopSender;

impl Sender for NoopSender {
    fn send(&self, _payload_json: &str) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_defaults_to_disabled_when_absent_or_malformed() {
        let dir = tempfile::tempdir().unwrap();
        // Absent file → default (off, not notified).
        let c = Consent::load(dir.path());
        assert_eq!(c.state, TelemetryState::Disabled);
        assert!(!c.notified);
        assert!(!c.is_enabled());
        // Malformed file → still the safe default, never enabled.
        std::fs::write(dir.path().join(CONSENT_FILE), "{ not json").unwrap();
        assert!(!Consent::load(dir.path()).is_enabled());
    }

    #[test]
    fn consent_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        Consent {
            state: TelemetryState::Enabled,
            notified: true,
        }
        .save(dir.path())
        .unwrap();
        let loaded = Consent::load(dir.path());
        assert!(loaded.is_enabled());
        assert!(loaded.notified);
    }

    #[test]
    fn env_parsing_matches_the_conventions() {
        // DO_NOT_TRACK / CI: set and not empty/"0" → true.
        assert!(set_nonzero(Some("1")));
        assert!(set_nonzero(Some("true")));
        assert!(!set_nonzero(None));
        assert!(!set_nonzero(Some("")));
        assert!(!set_nonzero(Some("0")));
        // PROMPTDUST_TELEMETRY kill-switch: falsy strings → true.
        assert!(falsy(Some("0")));
        assert!(falsy(Some("false")));
        assert!(falsy(Some("Off")));
        assert!(!falsy(Some("1")));
        assert!(!falsy(None));
    }

    #[test]
    fn is_active_requires_opt_in() {
        // A disabled consent is never active regardless of env.
        assert!(!is_active(&Consent::default()));
    }

    #[test]
    fn run_id_is_random_and_hex() {
        let a = run_id();
        let b = run_id();
        assert_eq!(a.len(), 32, "128 bits → 32 hex chars");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "each run gets a fresh id (not persisted)");
    }
}
