//! Golden parity for the dual endpoint score. The Rust scorers (`score::exposure_of` and
//! `score::assurance`) must reproduce the doctrine's reference numbers (`score_reference.py`)
//! exactly. The policy, reference, and scenarios are vendored under `tests/golden/scoring/`.
//! If an assertion here breaks, the scoring *model* changed — update `score::policy` and the
//! fixtures together, on purpose. See `tests/golden/scoring/README.md`.

use promptdust_core::{
    assurance, exposure_of, CoverageGap, EvasionSignal, EvidenceClass, ExposureInput,
    SensitivityType,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    scenarios: Vec<Scenario>,
}

#[derive(Deserialize)]
struct Scenario {
    name: String,
    expected_exposure: u32,
    expected_band: String,
    expected_assurance: u32,
    expected_assurance_band: String,
    #[serde(default)]
    coverage_gaps: Vec<CoverageGap>,
    #[serde(default)]
    evasion_signals: Vec<EvasionSignal>,
    corroborated_findings: u32,
    findings: Vec<GoldenFinding>,
}

#[derive(Deserialize)]
struct GoldenFinding {
    definition_id: String,
    base_weight: u8,
    evidence_class: EvidenceClass,
    age_days: Option<f64>,
    #[serde(default)]
    confirmed_types: Vec<SensitivityType>,
}

const FIXTURE: &str = include_str!("golden/scoring/scenarios.json");

#[test]
fn endpoint_scores_match_reference() {
    let fixture: Fixture = serde_json::from_str(FIXTURE).expect("scenarios.json parses");
    assert!(
        !fixture.scenarios.is_empty(),
        "fixture must contain scenarios"
    );

    for s in &fixture.scenarios {
        assert!(
            !s.findings.is_empty(),
            "scenario {} has no findings",
            s.name
        );
        let inputs: Vec<ExposureInput> = s
            .findings
            .iter()
            .map(|f| ExposureInput {
                definition_id: &f.definition_id,
                base_weight: f.base_weight,
                evidence_class: f.evidence_class,
                age_days: f.age_days,
                confirmed_types: &f.confirmed_types,
            })
            .collect();

        let got = exposure_of(&inputs);
        assert_eq!(
            got.score, s.expected_exposure,
            "scenario {:?}: Rust exposure {} != reference {}",
            s.name, got.score, s.expected_exposure
        );
        assert_eq!(
            got.band.as_str(),
            s.expected_band,
            "scenario {:?}: band {} != expected {}",
            s.name,
            got.band.as_str(),
            s.expected_band
        );

        // Assurance parity (the other half of the dual score).
        let asr = assurance(
            &s.coverage_gaps,
            &s.evasion_signals,
            s.corroborated_findings,
        );
        assert_eq!(
            asr.score, s.expected_assurance,
            "scenario {:?}: Rust assurance {} != reference {}",
            s.name, asr.score, s.expected_assurance
        );
        assert_eq!(
            asr.band.as_str(),
            s.expected_assurance_band,
            "scenario {:?}: assurance band {} != expected {}",
            s.name,
            asr.band.as_str(),
            s.expected_assurance_band
        );
    }
}

#[test]
fn recency_no_timestamp_and_floor() {
    // No timestamp → NO_TIMESTAMP_DEFAULT (0.6): content bw=10 → 10·1.0·0.6·1.0 = 6.0,
    // p = min(0.9, 6/12) = 0.5, exposure = 100·(1−0.5) = 50.
    let no_ts = exposure_of(&[ExposureInput {
        definition_id: "x",
        base_weight: 10,
        evidence_class: EvidenceClass::Content,
        age_days: None,
        confirmed_types: &[],
    }]);
    assert_eq!(no_ts.score, 50, "no-timestamp recency should be 0.6");

    // Recency floor 0.35: an ancient artifact decays to the floor, not to ~0.
    // content bw=10 → 10·1.0·0.35·1.0 = 3.5, p = 3.5/12 ≈ 0.2917, exposure = 29.
    let ancient = exposure_of(&[ExposureInput {
        definition_id: "x",
        base_weight: 10,
        evidence_class: EvidenceClass::Content,
        age_days: Some(100_000.0),
        confirmed_types: &[],
    }]);
    assert_eq!(ancient.score, 29, "recency must floor at 0.35, not vanish");
}

#[test]
fn dedup_takes_max_per_definition_not_sum() {
    // Two signals for ONE definition must collapse to the MAX contribution, never the sum.
    let strong = ExposureInput {
        definition_id: "dup",
        base_weight: 10,
        evidence_class: EvidenceClass::Content,
        age_days: Some(0.0),
        confirmed_types: &[],
    };
    let weak = ExposureInput {
        definition_id: "dup",
        base_weight: 10,
        evidence_class: EvidenceClass::Presence,
        age_days: Some(0.0),
        confirmed_types: &[],
    };

    let one = exposure_of(&[strong]);
    let both = exposure_of(&[strong, weak]);
    assert_eq!(
        one.score, both.score,
        "dedup MAX: adding a weaker same-definition signal must not raise the score"
    );

    // Order-independence proves MAX, not keep-first-seen: with the weaker signal listed
    // FIRST, a keep-first bug would retain the weaker contribution and score lower.
    let reversed = exposure_of(&[weak, strong]);
    assert_eq!(
        reversed.score, one.score,
        "dedup must keep the MAX contribution regardless of input order"
    );

    // Sanity: a *different* definition id does raise it (proves the dedup, not a no-op).
    let distinct = exposure_of(&[
        strong,
        ExposureInput {
            definition_id: "other",
            ..weak
        },
    ]);
    assert!(
        distinct.score > one.score,
        "a distinct definition must add exposure (else the dedup test is vacuous)"
    );
}

#[test]
fn absence_lowers_assurance_never_exposure() {
    use promptdust_core::{detect_assurance_signals, AssuranceInput, ContentStore, DiskEncryption};

    // The doctrine's hard rule (Charter §8.3): a content-capable tool that is present but
    // whose store is absent is NOT "clean". Exposure sees only the presence-level finding
    // (base_weight 8 × presence 0.3 = 2.4 → p 0.2 → 20); it is never inflated *or* deflated
    // by the missing content. The absence instead docks Assurance via an evasion signal.
    // Exposure and Assurance are computed by independent functions — absence cannot reach
    // Exposure at all.
    let presence_only = [ExposureInput {
        definition_id: "tool",
        base_weight: 8,
        evidence_class: EvidenceClass::Presence,
        age_days: Some(0.0),
        confirmed_types: &[],
    }];
    assert_eq!(
        exposure_of(&presence_only).score,
        20,
        "exposure is the presence-level contribution, not changed by the absence"
    );

    let (cov, eva) = detect_assurance_signals(
        DiskEncryption::Unknown,
        &[AssuranceInput {
            max_evidence_class: EvidenceClass::Content,
            present: true,
            content_store: ContentStore::Absent,
        }],
    );
    assert!(cov.is_empty());
    assert_eq!(eva, vec![EvasionSignal::AppPresentStoreAbsent]);
    assert_eq!(
        assurance(&cov, &eva, 0).score,
        88,
        "the absent store docks Assurance (evasion 12), not Exposure"
    );
}
