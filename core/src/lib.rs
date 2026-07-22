//! `promptdust-core` — the read-only AI-data footprint scanning engine.
//!
//! Discovers where AI tools store data on the local machine and reports what
//! amplifies the exposure. Constrained by hard invariants enforced by tests:
//!
//! - **Read-only** — never creates, modifies, moves, or deletes scanned files.
//! - **No network** — the scan path makes zero network calls.
//! - **Metadata-only (output-side)** — conversation content is never *emitted* in any
//!   output (reading bytes to derive a count is fine; emitting content is not — ADR-017).
//!
//! # Example
//!
//! ```no_run
//! use promptdust_core::{scan, ScanConfig};
//!
//! let cfg = ScanConfig::detect().expect("a home directory");
//! let result = scan(&cfg);
//! println!("{} findings", result.summary.total_findings);
//! ```

pub mod definitions;
pub mod detect;
pub mod inspect;
pub mod model;
pub mod output;
pub mod platform;
pub mod redact;
pub mod report;
pub mod score;

mod resolve;
mod scan;

pub use detect::Amplifier;
pub use inspect::Inspection;
pub use model::{
    Category, Confidence, Definition, EvidenceClass, Format, MatchKind, PathPattern, Platform,
    Sensitivity, SensitivityType, StorageEpoch, VersionDetect, Volatility,
};
pub use output::{DiagnosticsDocument, Host, OutputDocument};
pub use platform::{CloudRoot, DiskEncryption, SystemProbe, Tri};
pub use redact::RedactedSummary;
pub use report::{Finding, Mode, ScanResult, ScanWarning, Summary};
pub use scan::{scan, ScanConfig, DEFAULT_LARGE_THRESHOLD};
pub use score::{
    assurance, assurance_inputs, computed_for, detect_assurance_signals, exposure_of,
    interpretation, score, score_endpoint, AssuranceBand, AssuranceInput, AssuranceScore, Computed,
    ContentStore, CoverageGap, EvasionSignal, ExposureBand, ExposureInput, ExposureLevel,
    ExposureScore,
};

/// The **scan-output** contract version this build emits and understands.
///
/// This versions the JSON output document (`OutputDocument`) only. The definition
/// database is versioned independently — by each definition's own `schema_version`
/// field — and both happen to read `1` today. Bumped only on a breaking change to
/// the scan-output contract; additive fields (like `evidence_class`/`mode`) do not.
pub const SCHEMA_VERSION: u32 = 1;

/// Returns this crate's semantic version string (from `Cargo.toml`).
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_nonempty() {
        assert!(!version().is_empty(), "crate version must be reported");
    }

    #[test]
    fn schema_version_is_stable() {
        assert_eq!(SCHEMA_VERSION, 1);
    }
}
