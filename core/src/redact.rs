//! A path-scrubbed, count-only projection of a `ScanResult` — the shared, host-agnostic
//! core of every diagnostics bundle and (future) telemetry payload.
//!
//! A `ScanResult`'s findings carry absolute filesystem paths (`/Users/<name>/…`) that are
//! both **identifying** and **sensitive** (the exported report is treated as sensitive —
//! see `docs/PRIVACY.md`). Anything the app might *send* off the machine — a diagnostics
//! bundle a user pastes into a bug report, an opt-in anonymous telemetry payload — must be
//! built from this projection, never from the raw result.
//!
//! [`RedactedSummary`] emits only counts, versions, and structural facts: **no path, no
//! filename, no conversation content** (INV-3), and **nothing host-specific** — the core
//! stays host-agnostic, so a front-end adds `Host` (OS/arch) alongside this when it builds
//! the actual payload. Its canary/path-freedom is guarded by
//! `core/tests/invariants.rs::redacted_summary_is_path_and_canary_free`.
//!
//! The `by_tool`/`by_definition` keys are definition-*declared* labels (a tool display name, a
//! kebab-case id) — never derived from a matched path; for the bundled catalog they are
//! maintainer-controlled.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::platform::DiskEncryption;
use crate::report::{Mode, ScanResult};

/// A count-only, path-free view of a scan — safe to emit off-machine (with consent).
///
/// Every field is an aggregate count, a version, or a closed enum. Notably absent: any
/// `path`, filename, warning *text* (warning messages may embed a path, so only the
/// [`warning_count`](Self::warning_count) is kept), and any host detail.
#[derive(Debug, Clone, Serialize)]
pub struct RedactedSummary {
    /// Output-contract schema version.
    pub schema_version: u32,
    /// The definition-DB version used for the scan.
    pub definition_db_version: String,
    /// The consent/depth ring the scan ran at (`"inventory"` today).
    pub mode: Mode,
    /// Full-disk-encryption status at scan time — a global amplifier input, not identifying.
    pub disk_encryption: DiskEncryption,
    /// Total number of findings.
    pub total_findings: usize,
    /// Sum of all finding sizes in bytes.
    pub total_bytes: u64,
    /// Finding counts grouped by tool name.
    pub by_tool: BTreeMap<String, usize>,
    /// Finding counts grouped by exposure level.
    pub by_exposure: BTreeMap<String, usize>,
    /// Finding counts grouped by definition id — id + count only, **never** the matched path.
    pub by_definition: BTreeMap<String, usize>,
    /// Number of non-fatal warnings. Only the count is kept: a warning's `path`/`reason`
    /// can carry a real filesystem path, which must not leave the machine here.
    pub warning_count: usize,
}

impl RedactedSummary {
    /// Project a scan into its path-scrubbed, count-only summary.
    ///
    /// Reuses the already-computed [`Summary`](crate::report::Summary) for the tool/exposure
    /// tallies and derives the per-definition counts from the findings. Purely functional —
    /// reads the result, writes nothing.
    #[must_use]
    pub fn from_scan(result: &ScanResult) -> Self {
        let mut by_definition: BTreeMap<String, usize> = BTreeMap::new();
        for f in &result.findings {
            *by_definition.entry(f.definition_id.clone()).or_insert(0) += 1;
        }
        Self {
            schema_version: result.schema_version,
            definition_db_version: result.definition_db_version.clone(),
            mode: result.mode,
            disk_encryption: result.disk_encryption,
            total_findings: result.summary.total_findings,
            total_bytes: result.summary.total_bytes,
            by_tool: result.summary.by_tool.clone(),
            by_exposure: result.summary.by_exposure.clone(),
            by_definition,
            warning_count: result.warnings.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{ScanWarning, Summary};

    /// A finding-free `ScanResult` with the given warnings (findings are covered by the
    /// integration canary test; here we exercise the empty/warnings/scalar paths).
    fn result(definition_db_version: &str, warnings: Vec<ScanWarning>) -> ScanResult {
        let mut r = ScanResult {
            schema_version: crate::SCHEMA_VERSION,
            definition_db_version: definition_db_version.to_string(),
            disk_encryption: DiskEncryption::Unknown,
            mode: Mode::Inventory,
            exposure: None,
            assurance: None,
            interpretation: None,
            findings: vec![],
            warnings,
            summary: Summary::default(),
        };
        r.recompute_summary();
        r
    }

    #[test]
    fn empty_scan_projects_to_all_zero_counts() {
        let s = RedactedSummary::from_scan(&result("2026.07.1", vec![]));
        assert_eq!(s.total_findings, 0);
        assert_eq!(s.total_bytes, 0);
        assert!(s.by_tool.is_empty());
        assert!(s.by_exposure.is_empty());
        assert!(s.by_definition.is_empty());
        assert_eq!(s.warning_count, 0);
    }

    #[test]
    fn scalars_pass_through_and_warnings_are_counted_not_embedded() {
        let warnings = vec![
            ScanWarning::new_path("/Users/someone/secret/store", "permission denied"),
            ScanWarning::new_named("bad-sig", "malformed"),
        ];
        let s = RedactedSummary::from_scan(&result("2026.07.1", warnings));

        assert_eq!(s.schema_version, crate::SCHEMA_VERSION);
        assert_eq!(s.definition_db_version, "2026.07.1");
        assert!(matches!(s.mode, Mode::Inventory));
        assert!(matches!(s.disk_encryption, DiskEncryption::Unknown));

        // Warnings collapse to a count; neither the path nor the reason text may survive.
        assert_eq!(s.warning_count, 2);
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("/Users/someone/secret/store"));
        assert!(!json.contains("permission denied"));
    }
}
