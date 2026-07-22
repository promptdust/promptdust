//! The exposure model (spec §7): a deterministic, transparent, **informational**
//! ranking derived from baseline sensitivity plus fired amplifiers. It ranks
//! attention; it is never a pass/fail verdict.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::detect::Amplifier;
use crate::model::{EvidenceClass, Sensitivity, SensitivityType};
use crate::report::Finding;

mod policy;

mod assurance;
pub use assurance::{
    assurance, assurance_inputs, detect_assurance_signals, AssuranceBand, AssuranceInput,
    AssuranceScore, ContentStore, CoverageGap, EvasionSignal,
};

/// An informational exposure ranking. Ordered `Info < Low < Medium < High < Critical`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExposureLevel {
    /// Minimal attention warranted.
    Info,
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
    /// Highest attention warranted.
    Critical,
}

impl ExposureLevel {
    /// A stable lowercase identifier (also the summary bucket key).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

const fn sensitivity_base(s: Sensitivity) -> u32 {
    match s {
        Sensitivity::Low => 1,
        Sensitivity::Medium => 2,
        Sensitivity::High => 3,
    }
}

/// Additive amplifier weights. Kept in one place so tuning is a single reviewed change.
///
/// `BackupSwept` and `LargeGrowth` are **informational**: they are reported on the
/// finding but never raise the exposure level. `BackupSwept` fires on nearly every
/// file (a system backup includes almost everything) and its real risk is
/// encryption-dependent (a backup of a FileVault/BitLocker disk is itself encrypted),
/// so it is a persistence signal, not a confidentiality amplifier. `LargeGrowth` is
/// magnitude, not exposure.
const fn amplifier_weight(a: Amplifier) -> u32 {
    match a {
        Amplifier::CloudSync | Amplifier::InGitRepo => 2,
        Amplifier::WorldReadable | Amplifier::UnencryptedDisk => 1,
        Amplifier::BackupSwept | Amplifier::LargeGrowth => 0,
    }
}

/// Compute the exposure level from baseline sensitivity and fired amplifiers.
#[must_use]
pub fn score(sensitivity: Sensitivity, amplifiers: &[Amplifier]) -> ExposureLevel {
    let total: u32 = sensitivity_base(sensitivity)
        + amplifiers.iter().map(|&a| amplifier_weight(a)).sum::<u32>();
    match total {
        0 | 1 => ExposureLevel::Info,
        2 => ExposureLevel::Low,
        3 => ExposureLevel::Medium,
        4 => ExposureLevel::High,
        _ => ExposureLevel::Critical,
    }
}

// ── Endpoint Exposure (the doctrine's magnitude score) ──────────────────────────────
// A document-level 0–100 number, distinct from the per-finding `ExposureLevel` gradient
// above (which is unchanged). Faithful to scoring_model.yaml; golden-tested against
// score_reference.py. Metadata-only: reads base_weight / evidence class / recency. In the
// shipped path (`score_endpoint`) `content_factor` is 1.0 — no classifier ships, so
// `confirmed_types` is empty; `exposure_of` still computes it from `confirmed_types` (the
// golden tests exercise it) for when Ring 2 lands.

/// The endpoint Exposure band (magnitude of AI-data risk found). Never a verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExposureBand {
    /// 0–19.
    Minimal,
    /// 20–39.
    Low,
    /// 40–59.
    Moderate,
    /// 60–79.
    High,
    /// 80–100.
    Critical,
}

impl ExposureBand {
    const fn for_score(score: u32) -> Self {
        // Edges live in `policy` (the single tunable home); compare against the max of each
        // band in ascending order so the lower bounds never need restating here.
        if score <= policy::BAND_MINIMAL_MAX {
            Self::Minimal
        } else if score <= policy::BAND_LOW_MAX {
            Self::Low
        } else if score <= policy::BAND_MODERATE_MAX {
            Self::Moderate
        } else if score <= policy::BAND_HIGH_MAX {
            Self::High
        } else {
            Self::Critical
        }
    }

    /// A stable lowercase identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Moderate => "moderate",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

/// The endpoint Exposure score: a magnitude (0–100) plus its band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExposureScore {
    /// 0–100.
    pub score: u32,
    /// The band `score` falls in.
    pub band: ExposureBand,
}

/// One finding's metadata inputs to the endpoint Exposure score — a view of *metadata
/// only*; no content is read to build it.
#[derive(Debug, Clone, Copy)]
pub struct ExposureInput<'a> {
    /// Correlation key: inputs sharing this collapse to one finding (MAX contribution).
    pub definition_id: &'a str,
    /// The definition's declared base weight.
    pub base_weight: u8,
    /// The strongest evidence class reached for this finding.
    pub evidence_class: EvidenceClass,
    /// Artifact age in days, or `None` if no timestamp is known.
    pub age_days: Option<f64>,
    /// Confirmed sensitive-data types (empty until a classifier ships — Ring 2).
    pub confirmed_types: &'a [SensitivityType],
}

fn recency_mult(age_days: Option<f64>) -> f64 {
    match age_days {
        None => policy::NO_TIMESTAMP_DEFAULT,
        // Intentionally unclamped for negative ages (future / clock-skewed mtime), to stay
        // byte-faithful to the doctrine reference; callers pass non-negative ages in practice.
        Some(days) => policy::RECENCY_FLOOR.max(0.5_f64.powf(days / policy::HALF_LIFE_DAYS)),
    }
}

fn content_factor(evidence_class: EvidenceClass, confirmed: &[SensitivityType]) -> f64 {
    if evidence_class != EvidenceClass::Content {
        return 1.0;
    }
    // Content-class with no confirmed classification is scored at its *potential* (1.0);
    // otherwise the max weight over confirmed types wins.
    confirmed
        .iter()
        .map(|&t| policy::confirmed_sensitivity_weight(t))
        .fold(1.0, f64::max)
}

fn contribution(input: &ExposureInput) -> f64 {
    f64::from(input.base_weight)
        * policy::evidence_class_multiplier(input.evidence_class)
        * recency_mult(input.age_days)
        * content_factor(input.evidence_class, input.confirmed_types)
}

/// Endpoint Exposure from a set of finding inputs: dedup MAX by `definition_id`, then a
/// saturating noisy-OR aggregate (diminishing returns; no many-small-findings runaway).
#[must_use]
pub fn exposure_of(inputs: &[ExposureInput]) -> ExposureScore {
    // Correlate: one contribution per definition_id, taking the MAX (never the sum).
    let mut by_sig: BTreeMap<&str, f64> = BTreeMap::new();
    for input in inputs {
        let c = contribution(input);
        let slot = by_sig.entry(input.definition_id).or_insert(0.0);
        *slot = slot.max(c);
    }
    // Noisy-OR: p_i = min(cap, contribution / normalizer); exposure = 100·(1 − ∏(1 − p_i)).
    let product = by_sig.values().fold(1.0, |acc, &c| {
        let p = (c / policy::NORMALIZER).min(policy::PER_FINDING_CAP);
        acc * (1.0 - p)
    });
    // `round_ties_even` (banker's rounding) matches Python's `round()` in the reference, so
    // values landing exactly on N.5 agree with the doctrine (plain `.round()` would not).
    let score = (100.0 * (1.0 - product)).round_ties_even() as u32;
    ExposureScore {
        score,
        band: ExposureBand::for_score(score),
    }
}

/// Definitions declare `base_weight` on the doctrine's **0–10** scale, but the bundled DB
/// currently carries **0–100**; divide by this (rounding to nearest, so a `75` maps to `8`,
/// not `7`) when feeding the endpoint scorer so content stores don't saturate. See
/// `report::Finding`.
const BASE_WEIGHT_RESCALE: f64 = 10.0;

/// Build one finding's endpoint-Exposure input from its metadata + a front-end `now_epoch`
/// (core stays clockless). `base_weight` is rescaled to the doctrine's 0–10 scale;
/// `confirmed_types` is empty (no classifier ships, so `content_factor` stays 1.0).
fn exposure_input_of<'a>(f: &'a Finding, now_epoch: i64) -> ExposureInput<'a> {
    ExposureInput {
        definition_id: &f.definition_id,
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        base_weight: f
            .base_weight
            .map_or(0, |b| (f64::from(b) / BASE_WEIGHT_RESCALE).round() as u8),
        evidence_class: f.evidence_class,
        // A timestamp that overflows i64 degrades to "unknown age" (the conservative
        // no-timestamp weight), never a false "brand new". A future / clock-skewed mtime
        // clamps to age 0 (recency ≤ 1.0) so it can never inflate above `base_weight`
        // (and never produces a super-unity recency or a `0 × ∞` NaN).
        age_days: f.modified_epoch_secs.and_then(|m| {
            let m = i64::try_from(m).ok()?;
            Some(((now_epoch - m) as f64 / 86_400.0).max(0.0))
        }),
        confirmed_types: &[],
    }
}

/// Per-finding exposure intermediates — every factor behind a finding's endpoint
/// contribution, so the endpoint score is re-derivable from the findings alone
/// (Charter A6 explainability). Metadata-only.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Computed {
    /// Evidence-class multiplier applied to `base_weight` (presence/usage/content).
    pub class_mult: f64,
    /// Recency multiplier (age decay).
    pub recency_mult: f64,
    /// Content factor (1.0 until a classifier ships — Ring 2).
    pub content_factor: f64,
    /// Raw contribution: `base_weight × class_mult × recency_mult × content_factor`.
    pub contribution: f64,
    /// This finding's noisy-OR probability: `min(cap, contribution / normalizer)`.
    pub p: f64,
}

/// The re-derivable factors behind one exposure input's contribution.
#[must_use]
pub fn computed_of(input: &ExposureInput) -> Computed {
    let class_mult = policy::evidence_class_multiplier(input.evidence_class);
    let rmult = recency_mult(input.age_days);
    let cfactor = content_factor(input.evidence_class, input.confirmed_types);
    let contribution = f64::from(input.base_weight) * class_mult * rmult * cfactor;
    let p = (contribution / policy::NORMALIZER).min(policy::PER_FINDING_CAP);
    Computed {
        class_mult,
        recency_mult: rmult,
        content_factor: cfactor,
        contribution,
        p,
    }
}

/// One finding's re-derivable exposure factors, from its metadata + a front-end `now_epoch`.
#[must_use]
pub fn computed_for(f: &Finding, now_epoch: i64) -> Computed {
    computed_of(&exposure_input_of(f, now_epoch))
}

/// Endpoint Exposure for a scan's findings. `now_epoch` (Unix seconds) is supplied by the
/// front-end — core stays clockless — so recency can be computed. `content_factor` is 1.0
/// today (no classifier), so `confirmed_types` is empty for every finding; `base_weight` is
/// rescaled to the doctrine's 0–10 scale. The formula and aggregation are golden-tested
/// independently in `tests/scoring_golden.rs` (on already-0–10 inputs).
#[must_use]
pub fn score_endpoint(findings: &[Finding], now_epoch: i64) -> ExposureScore {
    let inputs: Vec<ExposureInput> = findings
        .iter()
        .map(|f| exposure_input_of(f, now_epoch))
        .collect();
    exposure_of(&inputs)
}

/// The plain-English reading of the two scores together (`scoring_model.yaml`
/// `interpretation_matrix`, reworded off "clean" per FR-5). Never a verdict: a low Exposure
/// with low Assurance is *not* "nothing here" — it is a blind or evaded look.
#[must_use]
pub fn interpretation(exposure: ExposureBand, assurance: AssuranceBand) -> &'static str {
    // "High exposure" is the doctrine's High+ band (score ≥ 60), matching score_reference.py's
    // `hi_e = exp >= 60` — not merely Moderate. (Moderate here would over-alarm.)
    let exposure_high = matches!(exposure, ExposureBand::High | ExposureBand::Critical);
    let assurance_high = matches!(assurance, AssuranceBand::High);
    match (exposure_high, assurance_high) {
        (true, true) => "Confirmed exposure — act now.",
        (true, false) => "Likely exposure, obscured — investigate.",
        (false, true) => "Low exposure, well-covered — nothing notable surfaced.",
        (false, false) => {
            "Low exposure but low assurance — possibly evaded or blind; investigate before trusting."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::Amplifier::{
        BackupSwept, CloudSync, InGitRepo, LargeGrowth, UnencryptedDisk, WorldReadable,
    };

    #[test]
    fn exposure_bands_map_at_their_boundaries() {
        // The five endpoint-Exposure bands and their exact edges (scoring_model.yaml).
        for (score, band) in [
            (0, ExposureBand::Minimal),
            (19, ExposureBand::Minimal),
            (20, ExposureBand::Low),
            (39, ExposureBand::Low),
            (40, ExposureBand::Moderate),
            (59, ExposureBand::Moderate),
            (60, ExposureBand::High),
            (79, ExposureBand::High),
            (80, ExposureBand::Critical),
            (100, ExposureBand::Critical),
        ] {
            assert_eq!(
                ExposureBand::for_score(score),
                band,
                "band for score {score}"
            );
        }
    }

    #[test]
    fn empty_findings_score_zero() {
        // No findings → a defined floor of 0/minimal, never a panic or NaN.
        let s = exposure_of(&[]);
        assert_eq!(s.score, 0);
        assert_eq!(s.band, ExposureBand::Minimal);
    }

    /// The exposure score of a single finding — a compact oracle for the per-finding factors.
    fn score1(
        base_weight: u8,
        evidence_class: EvidenceClass,
        age_days: Option<f64>,
        confirmed_types: &[SensitivityType],
    ) -> u32 {
        exposure_of(&[ExposureInput {
            definition_id: "s",
            base_weight,
            evidence_class,
            age_days,
            confirmed_types,
        }])
        .score
    }

    #[test]
    fn content_factor_weights_and_max_over_confirmed() {
        use EvidenceClass::Content;
        use SensitivityType::{Secret, Source};
        // A single non-saturating content store (bw 3, age 0), so the factor is visible.
        // Potential (no confirmed) → factor 1.0 → 3.0/12 = 0.25 → 25.
        assert_eq!(score1(3, Content, Some(0.0), &[]), 25);
        // Confirmed secret (1.6) → 4.8/12 = 0.40 → 40 (pins the secret weight).
        assert_eq!(score1(3, Content, Some(0.0), &[Secret]), 40);
        // Confirmed source (1.15) → 3.45/12 = 0.2875 → 29 (pins the source weight).
        assert_eq!(score1(3, Content, Some(0.0), &[Source]), 29);
        // MAX over confirmed: [secret, source] takes 1.6 (secret), not 1.15 → 40, order-free.
        assert_eq!(score1(3, Content, Some(0.0), &[Secret, Source]), 40);
        assert_eq!(score1(3, Content, Some(0.0), &[Source, Secret]), 40);
    }

    #[test]
    fn non_content_evidence_ignores_confirmed_types() {
        use SensitivityType::Secret;
        // content_factor short-circuits to 1.0 off the content class: a presence finding
        // with a confirmed secret scores exactly as it would with none.
        let with = score1(10, EvidenceClass::Presence, Some(0.0), &[Secret]);
        let without = score1(10, EvidenceClass::Presence, Some(0.0), &[]);
        assert_eq!(
            with, without,
            "confirmed types must not affect a non-content finding"
        );
    }

    #[test]
    fn recency_half_life_scales_the_weight() {
        // At exactly one half-life (180 d) recency = 0.5: bw 12 content → 12·0.5 = 6 →
        // 6/12 = 0.5 → 50. A different HALF_LIFE_DAYS moves this.
        assert_eq!(score1(12, EvidenceClass::Content, Some(180.0), &[]), 50);
    }

    #[test]
    fn per_finding_cap_limits_a_single_contribution() {
        // Raw p would be 12/12 = 1.0; the 0.9 cap holds it to 90, not 100.
        assert_eq!(score1(12, EvidenceClass::Content, Some(0.0), &[]), 90);
    }

    #[test]
    fn rounding_is_half_to_even() {
        // bw 5 presence age 0 → 1.5 → 1.5/12 = 0.125 → 100·0.125 = 12.5 exactly. Banker's
        // rounding gives 12 (to even); round-half-away would wrongly give 13.
        assert_eq!(score1(5, EvidenceClass::Presence, Some(0.0), &[]), 12);
    }

    fn finding_at(
        definition_id: &str,
        base_weight: Option<u8>,
        evidence_class: EvidenceClass,
        modified_epoch_secs: Option<u64>,
    ) -> Finding {
        use crate::model::{Category, Format};
        Finding {
            definition_id: definition_id.into(),
            tool: "t".into(),
            category: Category::Transcript,
            format: Format::Jsonl,
            path: std::path::PathBuf::from("/x"),
            size_bytes: 0,
            file_count: 1,
            modified_epoch_secs,
            inspection: None,
            amplifiers: Vec::new(),
            amplifier_detail: serde_json::json!({}),
            sensitivity: Sensitivity::High,
            evidence_class,
            base_weight,
            exposure_level: ExposureLevel::Info,
            computed: None,
            why: "w".into(),
            guidance: Vec::new(),
            confidence: None,
        }
    }

    #[test]
    fn score_endpoint_maps_finding_metadata_to_inputs() {
        let now = 1_000_000_000_i64;
        let day = 86_400_u64;
        // A 2-day-old content store and a 30-day-old presence cache. Finding base weights are
        // on the bundled 0–100 scale; score_endpoint rescales ÷10 to the doctrine's 0–10.
        let findings = vec![
            finding_at(
                "a",
                Some(80),
                EvidenceClass::Content,
                Some(now as u64 - 2 * day),
            ),
            finding_at(
                "b",
                Some(30),
                EvidenceClass::Presence,
                Some(now as u64 - 30 * day),
            ),
        ];
        let got = score_endpoint(&findings, now);

        // Equals the same inputs scored directly (rescaled base weights 8 and 3) — age_days
        // derived from mtime + now_epoch, whole-day ages, no confirmed types (factor 1.0).
        let want = exposure_of(&[
            ExposureInput {
                definition_id: "a",
                base_weight: 8,
                evidence_class: EvidenceClass::Content,
                age_days: Some(2.0),
                confirmed_types: &[],
            },
            ExposureInput {
                definition_id: "b",
                base_weight: 3,
                evidence_class: EvidenceClass::Presence,
                age_days: Some(30.0),
                confirmed_types: &[],
            },
        ]);
        assert_eq!(
            got, want,
            "score_endpoint must derive age_days from mtime + now_epoch"
        );

        // No base_weight → zero contribution.
        let no_bw = vec![finding_at(
            "c",
            None,
            EvidenceClass::Content,
            Some(now as u64),
        )];
        assert_eq!(
            score_endpoint(&no_bw, now).score,
            0,
            "missing base_weight → zero contribution"
        );

        // A timestamp that overflows i64 degrades to the conservative no-timestamp recency
        // (0.6), not a false age 0: bw 100 rescales to 10, content → 10·1.0·0.6·1.0 = 6 →
        // p 0.5 → score 50.
        let overflow = vec![finding_at(
            "d",
            Some(100),
            EvidenceClass::Content,
            Some(u64::MAX),
        )];
        assert_eq!(
            score_endpoint(&overflow, now).score,
            50,
            "overflowing mtime → no-timestamp recency, never a false brand-new"
        );
    }

    #[test]
    fn base_weight_rescale_rounds_to_nearest() {
        // Bundled weights are 0–100; a 75 must round to 8 (not floor to 7). Content, age 0:
        // 8·1·1·1 = 8 → p 8/12 ≈ 0.667 → 67. Flooring to 7 would wrongly give 58.
        let now = 1_000_000_000_i64;
        let f = finding_at("s", Some(75), EvidenceClass::Content, Some(now as u64));
        assert_eq!(score_endpoint(&[f], now).score, 67);
    }

    #[test]
    fn computed_reproduces_the_endpoint_aggregate() {
        // Charter A6: the endpoint Exposure must be re-derivable from the per-finding
        // computed{} factors alone (MAX p per definition_id, then noisy-OR).
        let now = 1_000_000_000_i64;
        let day = 86_400_u64;
        let findings = vec![
            finding_at(
                "a",
                Some(80),
                EvidenceClass::Content,
                Some(now as u64 - 2 * day),
            ),
            // Same definition_id, smaller contribution → dedup keeps the MAX, not the sum.
            finding_at("a", Some(50), EvidenceClass::Content, Some(now as u64)),
            finding_at(
                "b",
                Some(30),
                EvidenceClass::Presence,
                Some(now as u64 - 30 * day),
            ),
        ];
        let endpoint = score_endpoint(&findings, now).score;

        let mut max_p: BTreeMap<&str, f64> = BTreeMap::new();
        for f in &findings {
            let c = computed_for(f, now);
            let slot = max_p.entry(f.definition_id.as_str()).or_insert(0.0);
            *slot = slot.max(c.p);
        }
        let product = max_p.values().fold(1.0, |acc, &p| acc * (1.0 - p));
        let rederived = (100.0 * (1.0 - product)).round_ties_even() as u32;
        assert_eq!(
            endpoint, rederived,
            "endpoint Exposure must be re-derivable from per-finding computed.p"
        );
    }

    #[test]
    fn baseline_without_amplifiers() {
        assert_eq!(score(Sensitivity::Low, &[]), ExposureLevel::Info); // 1
        assert_eq!(score(Sensitivity::Medium, &[]), ExposureLevel::Low); // 2
        assert_eq!(score(Sensitivity::High, &[]), ExposureLevel::Medium); // 3
    }

    #[test]
    fn amplifiers_raise_the_level() {
        // high(3) + cloud(2) = 5 → critical
        assert_eq!(
            score(Sensitivity::High, &[CloudSync]),
            ExposureLevel::Critical
        );
        // high(3) + git(2) = 5 → critical
        assert_eq!(
            score(Sensitivity::High, &[InGitRepo]),
            ExposureLevel::Critical
        );
        // medium(2) + world(1) = 3 → medium
        assert_eq!(
            score(Sensitivity::Medium, &[WorldReadable]),
            ExposureLevel::Medium
        );
        // low(1) + backup(0, informational) + disk(1) = 2 → low
        assert_eq!(
            score(Sensitivity::Low, &[BackupSwept, UnencryptedDisk]),
            ExposureLevel::Low
        );
    }

    #[test]
    fn informational_amplifiers_never_raise_the_level() {
        // Neither large_growth nor backup_swept changes the level (both weight 0).
        assert_eq!(
            score(Sensitivity::High, &[LargeGrowth]),
            ExposureLevel::Medium
        );
        // high(3) + backup(0) = 3 → medium (a plaintext transcript at rest on an
        // encrypted disk is Medium, not pushed to High by being in a backup).
        assert_eq!(
            score(Sensitivity::High, &[BackupSwept]),
            ExposureLevel::Medium
        );
        // Adding a real amplifier still counts: high(3) + world(1) + backup(0) = 4 → high.
        assert_eq!(
            score(Sensitivity::High, &[WorldReadable, BackupSwept]),
            ExposureLevel::High
        );
    }

    #[test]
    fn ordering_is_ascending() {
        assert!(ExposureLevel::Info < ExposureLevel::Low);
        assert!(ExposureLevel::High < ExposureLevel::Critical);
    }

    #[test]
    fn exhaustive_score_table_is_stable() {
        // Guard the full mapping so weight changes are deliberate.
        let cases = [
            (Sensitivity::Low, vec![], ExposureLevel::Info),
            (Sensitivity::Medium, vec![], ExposureLevel::Low),
            (Sensitivity::High, vec![], ExposureLevel::Medium),
            (Sensitivity::High, vec![WorldReadable], ExposureLevel::High),
            (
                Sensitivity::High,
                vec![CloudSync, WorldReadable],
                ExposureLevel::Critical,
            ),
            (Sensitivity::Low, vec![WorldReadable], ExposureLevel::Low),
        ];
        for (s, amps, expected) in cases {
            assert_eq!(score(s, &amps), expected, "case {s:?} {amps:?}");
        }
    }
}
