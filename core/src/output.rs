//! The front-end output document — the versioned JSON contract.
//!
//! The core stays clockless and host-agnostic: the caller supplies `generated_at`
//! and host details. Both the CLI and the desktop app build their output through
//! this one type so the contract has a single definition.

use serde::Serialize;

use crate::platform::DiskEncryption;
use crate::redact::RedactedSummary;
use crate::report::{Finding, Mode, ScanResult, ScanWarning, Summary};
use crate::score::ExposureScore;

/// Host details for the report header (all caller-provided).
#[derive(Debug, Clone, Serialize)]
pub struct Host {
    /// Operating system (e.g. `macos`).
    pub os: String,
    /// CPU architecture (e.g. `aarch64`).
    pub arch: String,
    /// OS version string, if the caller could determine one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
}

/// The endpoint Assurance, projected for the output (its coverage/evasion evidence is hoisted
/// to the document's top-level `coverage_gaps` / `evasion_signals` arrays).
#[derive(Debug, Serialize)]
struct AssuranceOut {
    score: u32,
    band: &'static str,
    corroboration_bonus: u32,
}

/// One coverage gap or evasion signal, rendered with its stable id, human note, and the
/// Assurance points it docked — so the Assurance score is re-derivable from the output alone.
#[derive(Debug, Serialize)]
struct SignalOut {
    id: &'static str,
    note: &'static str,
    penalty: u32,
}

/// The complete scan report as emitted to `--json` / `--output` / the GUI.
#[derive(Debug, Serialize)]
pub struct OutputDocument<'a> {
    schema_version: u32,
    generated_at: String,
    host: Host,
    definition_db_version: &'a str,
    disk_encryption: DiskEncryption,
    mode: Mode,
    /// Endpoint Exposure (magnitude). Absent on an unscored result.
    #[serde(skip_serializing_if = "Option::is_none")]
    exposure: Option<ExposureScore>,
    /// Endpoint Assurance (trust in the Exposure number). Absent on an unscored result.
    #[serde(skip_serializing_if = "Option::is_none")]
    assurance: Option<AssuranceOut>,
    /// Plain-English reading of the two scores together. Absent on an unscored result.
    #[serde(skip_serializing_if = "Option::is_none")]
    interpretation: Option<&'static str>,
    /// What the scan could not see (each docked Assurance).
    coverage_gaps: Vec<SignalOut>,
    /// Evidence of deliberate cleanup (each docked Assurance, never lowered Exposure).
    evasion_signals: Vec<SignalOut>,
    /// Findings backed by ≥2 independent signal classes — inert until later rings track
    /// signal classes; the shape is present so the contract is stable.
    corroborations: Vec<SignalOut>,
    warnings: &'a [ScanWarning],
    findings: &'a [Finding],
    summary: &'a Summary,
}

impl<'a> OutputDocument<'a> {
    /// Wrap a [`ScanResult`] with a caller-supplied timestamp and host. If the result was
    /// scored (front-end supplied `now_epoch`), the dual score + interpretation + signal
    /// arrays are emitted; otherwise they are absent (the shape degrades cleanly).
    #[must_use]
    pub fn new(result: &'a ScanResult, generated_at: String, host: Host) -> Self {
        let (assurance, coverage_gaps, evasion_signals) = match &result.assurance {
            Some(a) => (
                Some(AssuranceOut {
                    score: a.score,
                    band: a.band.as_str(),
                    corroboration_bonus: a.corroboration_bonus,
                }),
                a.coverage_gaps
                    .iter()
                    .map(|g| SignalOut {
                        id: g.id(),
                        note: g.note(),
                        penalty: g.penalty(),
                    })
                    .collect(),
                a.evasion_signals
                    .iter()
                    .map(|e| SignalOut {
                        id: e.id(),
                        note: e.note(),
                        penalty: e.penalty(),
                    })
                    .collect(),
            ),
            None => (None, Vec::new(), Vec::new()),
        };
        Self {
            schema_version: result.schema_version,
            generated_at,
            host,
            definition_db_version: &result.definition_db_version,
            disk_encryption: result.disk_encryption,
            mode: result.mode,
            exposure: result.exposure,
            assurance,
            interpretation: result.interpretation,
            coverage_gaps,
            evasion_signals,
            corroborations: Vec::new(),
            warnings: &result.warnings,
            findings: &result.findings,
            summary: &result.summary,
        }
    }

    /// Serialize as pretty JSON (never fails; falls back to `{}`).
    #[must_use]
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// A redacted, count-only diagnostics bundle a user can inspect and paste into a bug
/// report. Built from the path-scrubbed [`RedactedSummary`](crate::RedactedSummary) — never
/// the raw findings — plus a caller-supplied timestamp, tool version, and [`Host`]. It
/// carries no path, filename, conversation content, or host detail beyond OS/arch; its
/// canary/path-freedom is guarded by
/// `core/tests/invariants.rs::diagnostics_document_is_path_and_canary_free`.
#[derive(Debug, Serialize)]
pub struct DiagnosticsDocument {
    /// Stable marker identifying this as the redacted diagnostics bundle.
    kind: &'static str,
    /// When the bundle was produced (caller-supplied; the core is clockless).
    generated_at: String,
    /// The `promptdust` version that produced it (caller-supplied).
    tool_version: String,
    /// Host OS/arch/version (caller-supplied).
    host: Host,
    /// Wall-clock scan duration in milliseconds, if the caller measured it.
    #[serde(skip_serializing_if = "Option::is_none")]
    scan_duration_ms: Option<u64>,
    /// The path-scrubbed, count-only projection of the scan.
    summary: RedactedSummary,
}

impl DiagnosticsDocument {
    /// Assemble the bundle from a scan plus caller-supplied host/version/timestamp/duration.
    /// The scan is projected through [`RedactedSummary`] so no path or content can appear.
    #[must_use]
    pub fn new(
        result: &ScanResult,
        generated_at: String,
        tool_version: String,
        host: Host,
        scan_duration_ms: Option<u64>,
    ) -> Self {
        Self {
            kind: "promptdust-diagnostics",
            generated_at,
            tool_version,
            host,
            scan_duration_ms,
            summary: RedactedSummary::from_scan(result),
        }
    }

    /// Serialize as pretty JSON (never fails; falls back to `{}`).
    #[must_use]
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_result() -> ScanResult {
        let mut r = ScanResult {
            schema_version: crate::SCHEMA_VERSION,
            definition_db_version: "2026.07.0".to_string(),
            disk_encryption: DiskEncryption::Unknown,
            mode: Mode::Inventory,
            exposure: None,
            assurance: None,
            interpretation: None,
            findings: vec![],
            warnings: vec![],
            summary: Summary::default(),
        };
        r.recompute_summary();
        r
    }

    #[test]
    fn diagnostics_document_is_count_only_and_versioned() {
        let result = empty_result();
        let doc = DiagnosticsDocument::new(
            &result,
            "2026-07-15T00:00:00Z".to_string(),
            "9.9.9".to_string(),
            Host {
                os: "macos".to_string(),
                arch: "aarch64".to_string(),
                os_version: Some("26.5".to_string()),
            },
            Some(42),
        );
        let json: serde_json::Value = serde_json::from_str(&doc.to_json_pretty()).unwrap();
        assert_eq!(json["kind"], "promptdust-diagnostics");
        assert_eq!(json["tool_version"], "9.9.9");
        assert_eq!(json["generated_at"], "2026-07-15T00:00:00Z");
        assert_eq!(json["host"]["arch"], "aarch64");
        assert_eq!(json["scan_duration_ms"], 42);
        // The embedded summary is the count-only projection; raw findings never surface.
        assert_eq!(json["summary"]["total_findings"], 0);
        assert!(
            json.get("findings").is_none(),
            "diagnostics never carries raw findings"
        );

        // scan_duration_ms is omitted when the caller didn't measure it.
        let doc2 = DiagnosticsDocument::new(
            &result,
            "t".to_string(),
            "9.9.9".to_string(),
            Host {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                os_version: None,
            },
            None,
        );
        let json2: serde_json::Value = serde_json::from_str(&doc2.to_json_pretty()).unwrap();
        assert!(
            json2.get("scan_duration_ms").is_none(),
            "duration omitted when None"
        );
    }

    #[test]
    fn document_carries_generated_at_and_host() {
        let result = empty_result();
        let doc = OutputDocument::new(
            &result,
            "2026-07-15T00:00:00Z".to_string(),
            Host {
                os: "macos".to_string(),
                arch: "aarch64".to_string(),
                os_version: Some("26.5".to_string()),
            },
        );
        let json: serde_json::Value = serde_json::from_str(&doc.to_json_pretty()).unwrap();
        assert_eq!(json["schema_version"], crate::SCHEMA_VERSION);
        assert_eq!(json["generated_at"], "2026-07-15T00:00:00Z");
        assert_eq!(json["host"]["os"], "macos");
        assert_eq!(json["definition_db_version"], "2026.07.0");
        // Additive output-contract fields (SCHEMA_VERSION stays 1).
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["mode"], "inventory");
        // Unscored: the dual score is absent, but the signal arrays are always present.
        assert!(json.get("exposure").is_none());
        assert!(json.get("assurance").is_none());
        assert!(json["coverage_gaps"].is_array());
        assert!(json["corroborations"].is_array());
    }

    #[test]
    fn scored_document_carries_the_dual_number_and_interpretation() {
        let mut result = empty_result();
        // No findings, unknown disk → Exposure 0/minimal, Assurance full/high.
        result.score(1_000_000_000, |_| None);
        let doc = OutputDocument::new(
            &result,
            "2026-07-15T00:00:00Z".to_string(),
            Host {
                os: "macos".to_string(),
                arch: "aarch64".to_string(),
                os_version: None,
            },
        );
        let json: serde_json::Value = serde_json::from_str(&doc.to_json_pretty()).unwrap();
        assert_eq!(json["exposure"]["score"], 0);
        assert_eq!(json["exposure"]["band"], "minimal");
        assert_eq!(json["assurance"]["score"], 100);
        assert_eq!(json["assurance"]["band"], "high");
        assert_eq!(
            json["interpretation"],
            "Low exposure, well-covered — nothing notable surfaced."
        );
    }
}
