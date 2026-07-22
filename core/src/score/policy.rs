//! Endpoint-Exposure scoring **policy** — a faithful, deterministic transcription of
//! `scoring_model.yaml` (vendored under `core/tests/golden/scoring/`). Kept separate from
//! the fact base (definitions) so thresholds are tunable and golden-testable without
//! touching shared facts. Changing a constant here is a scoring-model release, not a
//! silent edit — the golden parity test guards it.

use crate::model::{EvidenceClass, SensitivityType};

/// Recency half-life in days (exponential decay of a finding's weight).
pub(super) const HALF_LIFE_DAYS: f64 = 180.0;
/// Recency floor — decay re-weights, it never erases (old findings stay alive).
pub(super) const RECENCY_FLOOR: f64 = 0.35;
/// Recency multiplier when a finding has no timestamp (conservative middle weight).
pub(super) const NO_TIMESTAMP_DEFAULT: f64 = 0.6;
/// Divides a raw contribution into a probability-like value for noisy-OR aggregation.
pub(super) const NORMALIZER: f64 = 12.0;
/// No single finding may contribute more than this to the noisy-OR product.
pub(super) const PER_FINDING_CAP: f64 = 0.9;

/// Endpoint-Exposure band upper edges (inclusive), from `scoring_model.yaml`
/// `scores.exposure.bands`. A score `≤ MINIMAL_MAX` is minimal, `≤ LOW_MAX` is low, and
/// so on; anything above `HIGH_MAX` is critical.
pub(super) const BAND_MINIMAL_MAX: u32 = 19;
pub(super) const BAND_LOW_MAX: u32 = 39;
pub(super) const BAND_MODERATE_MAX: u32 = 59;
pub(super) const BAND_HIGH_MAX: u32 = 79;

// ── Assurance model (scoring_model.yaml `assurance` + `correlation.corroboration`) ──
// assurance = clamp(base − coverage − evasion + corroboration, 0, 100).
// The scalar knobs are here; the per-gap / per-signal penalties are transcribed alongside
// their `id`/`note` on the `CoverageGap` / `EvasionSignal` catalogs in `assurance.rs` (one
// unit per `scoring_model.yaml` catalog entry).

/// Assurance starts at full trust; it is docked for what could not be seen and for signs
/// of cleanup, and credited for corroboration.
pub(super) const ASSURANCE_BASE: u32 = 100;
/// Total coverage-gap penalty is capped (blindness alone never zeroes trust).
pub(super) const COVERAGE_PENALTY_CAP: u32 = 50;
/// Total evasion-signal penalty is capped.
pub(super) const EVASION_PENALTY_CAP: u32 = 45;
/// Assurance bonus per finding corroborated by ≥ 2 independent signal classes.
pub(super) const CORROBORATION_BONUS_PER_FINDING: u32 = 4;
/// Corroboration bonus cap.
pub(super) const CORROBORATION_BONUS_CAP: u32 = 20;
/// Assurance band upper edges (inclusive), from `scores.assurance.bands`: `≤ LOW_MAX` is
/// low (blind/evaded), `≤ PARTIAL_MAX` is partial; above is high (trustworthy).
pub(super) const ASSURANCE_BAND_LOW_MAX: u32 = 39;
pub(super) const ASSURANCE_BAND_PARTIAL_MAX: u32 = 69;

/// Evidence-class multiplier on `base_weight` (presence < usage < content).
pub(super) const fn evidence_class_multiplier(ec: EvidenceClass) -> f64 {
    match ec {
        EvidenceClass::Presence => 0.3,
        EvidenceClass::Usage => 0.6,
        EvidenceClass::Content => 1.0,
    }
}

/// Content factor for a *confirmed* sensitive-data type (max over confirmed types wins).
/// Only the types PromptDust's `SensitivityType` models; the policy also defines
/// pci/regulated/business weights, which have no enum variant yet.
pub(super) fn confirmed_sensitivity_weight(t: SensitivityType) -> f64 {
    match t {
        SensitivityType::Secret => 1.6,
        SensitivityType::Phi => 1.5,
        SensitivityType::Pii => 1.3,
        SensitivityType::Source => 1.15,
    }
}
