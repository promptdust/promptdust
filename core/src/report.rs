//! The scan output data model — the structured contract every front-end renders.
//!
//! The core is clockless: it never records the wall-clock scan time (that is stamped
//! by front-ends). File modification times are a property of the scanned file, so
//! they *are* recorded here.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;

use crate::detect::Amplifier;
use crate::inspect::Inspection;
use crate::model::{Category, Confidence, EvidenceClass, Format, Sensitivity};
use crate::platform::DiskEncryption;
use crate::score::{AssuranceScore, Computed, ExposureLevel, ExposureScore};

/// One matched artifact and everything known about it.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// The definition that matched.
    pub definition_id: String,
    /// Display name of the tool.
    pub tool: String,
    /// What the artifact contains.
    pub category: Category,
    /// On-disk format.
    pub format: Format,
    /// Absolute path to the artifact.
    pub path: PathBuf,
    /// Size in bytes (recursive for directories).
    pub size_bytes: u64,
    /// Number of files (1 for a file; recursive count for a directory).
    pub file_count: u64,
    /// Last-modified time as Unix epoch seconds, if available.
    pub modified_epoch_secs: Option<u64>,
    /// Metadata-only inspection results, if an inspector ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspection: Option<Inspection>,
    /// Exposure amplifiers that fired for this artifact.
    pub amplifiers: Vec<Amplifier>,
    /// Per-amplifier detail, keyed by amplifier name.
    pub amplifier_detail: Value,
    /// Baseline sensitivity.
    pub sensitivity: Sensitivity,
    /// The evidence depth reached for this finding (Ring 0 = `presence`), capped by
    /// the definition's `max_evidence_class`. Never `content` today.
    pub evidence_class: EvidenceClass,
    /// The definition's declared base weight — an **internal** input to the endpoint
    /// Exposure score (`score::score_endpoint`), read in-memory and never emitted (its
    /// values are not part of the output contract). Intended scale is the doctrine's
    /// 0–10 (see `score::policy`); bundled definitions currently declare 0–100 pending a
    /// re-scale, which is why the endpoint score is not surfaced yet. `None` = the
    /// definition omits it (no contribution).
    #[serde(skip)]
    pub base_weight: Option<u8>,
    /// Informational exposure ranking (never a verdict).
    pub exposure_level: ExposureLevel,
    /// Re-derivable endpoint-Exposure factors for this finding (Charter A6). Populated when
    /// the front-end supplies `now_epoch` to score the result; `None` on an unscored result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computed: Option<Computed>,
    /// One-sentence rationale.
    pub why: String,
    /// Non-destructive guidance.
    pub guidance: Vec<String>,
    /// Confidence tier, if declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

/// A non-fatal problem encountered during a scan (bad path, unreadable file, ...).
#[derive(Debug, Clone, Serialize)]
pub struct ScanWarning {
    /// The path involved, if any.
    pub path: Option<PathBuf>,
    /// Human-readable reason.
    pub reason: String,
}

impl ScanWarning {
    /// A warning tied to a filesystem path.
    #[must_use]
    pub fn new_path(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self {
            path: Some(path.into()),
            reason: reason.into(),
        }
    }

    /// A warning tied to a named source (e.g. a definition file), not a real path.
    #[must_use]
    pub fn new_named(source: &str, reason: impl Into<String>) -> Self {
        Self {
            path: None,
            reason: format!("{source}: {}", reason.into()),
        }
    }
}

/// Aggregate counts for a scan.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    /// Total number of findings.
    pub total_findings: usize,
    /// Sum of all finding sizes in bytes.
    pub total_bytes: u64,
    /// Findings grouped by tool name.
    pub by_tool: BTreeMap<String, usize>,
    /// Findings grouped by exposure level.
    pub by_exposure: BTreeMap<String, usize>,
}

/// The consent/depth ring a scan ran at. Only Ring 0 (`Inventory`) ships today;
/// deeper rings (usage, content classification, …) add variants as they land.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Ring 0: read-only inventory — presence, size, timestamps, counts, shape.
    #[default]
    Inventory,
}

impl Mode {
    /// A stable lowercase identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inventory => "inventory",
        }
    }

    /// The deepest evidence class this ring may reach — its evidence ceiling. A finding's
    /// evidence class is capped by this (Ring 0 = `Presence`), so no consent ring emits
    /// evidence deeper than the user opted into.
    #[must_use]
    pub const fn max_reach(self) -> EvidenceClass {
        match self {
            Self::Inventory => EvidenceClass::Presence,
        }
    }
}

/// The complete result of one scan.
#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    /// Output-contract schema version.
    pub schema_version: u32,
    /// The definition-DB version used.
    pub definition_db_version: String,
    /// Full-disk encryption status at scan time (a global amplifier input).
    pub disk_encryption: DiskEncryption,
    /// The consent/depth ring this scan ran at (core-stamped; Ring 0 today).
    pub mode: Mode,
    /// Endpoint Exposure — the magnitude of AI-data risk (0–100 + band). `None` until the
    /// front-end scores the result with a `now_epoch` (core stays clockless).
    pub exposure: Option<ExposureScore>,
    /// Endpoint Assurance — how much to trust the Exposure number (0–100 + band + the
    /// coverage gaps / evasion signals behind it). `None` on an unscored result.
    pub assurance: Option<AssuranceScore>,
    /// The plain-English reading of the two scores together. `None` on an unscored result.
    pub interpretation: Option<&'static str>,
    /// All matched artifacts.
    pub findings: Vec<Finding>,
    /// Non-fatal problems.
    pub warnings: Vec<ScanWarning>,
    /// Aggregate summary.
    pub summary: Summary,
}

impl ScanResult {
    /// Score the result at the document level. The front-end supplies `now_epoch` (Unix
    /// seconds) — core never reads the clock — and `content_max(id)`, a lookup of a
    /// definition's declared deepest evidence class (for the absence-rule). Attaches each
    /// finding's re-derivable `computed{}` (Charter A6), then computes endpoint Exposure,
    /// Assurance, and their plain-English interpretation. Idempotent; safe to call once.
    pub fn score(&mut self, now_epoch: i64, content_max: impl Fn(&str) -> Option<EvidenceClass>) {
        for f in &mut self.findings {
            f.computed = Some(crate::score::computed_for(f, now_epoch));
        }
        let exposure = crate::score::score_endpoint(&self.findings, now_epoch);
        let inputs = crate::score::assurance_inputs(&self.findings, content_max);
        let (coverage, evasion) =
            crate::score::detect_assurance_signals(self.disk_encryption, &inputs);
        let assurance = crate::score::assurance(&coverage, &evasion, 0);
        self.interpretation = Some(crate::score::interpretation(exposure.band, assurance.band));
        self.exposure = Some(exposure);
        self.assurance = Some(assurance);
    }

    /// Compute the summary from the current findings.
    pub(crate) fn recompute_summary(&mut self) {
        let mut summary = Summary {
            total_findings: self.findings.len(),
            ..Default::default()
        };
        for f in &self.findings {
            summary.total_bytes = summary.total_bytes.saturating_add(f.size_bytes);
            *summary.by_tool.entry(f.tool.clone()).or_insert(0) += 1;
            *summary
                .by_exposure
                .entry(f.exposure_level.as_str().to_string())
                .or_insert(0) += 1;
        }
        self.summary = summary;
    }
}
