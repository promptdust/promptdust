//! Explainability (A6): every finding must be traceable — a non-empty rationale, an
//! exposure level that recomputes from the emitted axes (not an opaque number), and a
//! 1:1 amplifier ↔ `amplifier_detail` mapping so every ranking input is inspectable.

mod common;

use promptdust_core::{scan, score, ScanConfig};
use std::fs;

#[test]
fn every_finding_is_explainable() {
    let home = tempfile::tempdir().unwrap();
    // A transcript inside BOTH a cloud-synced folder and a git working tree → ≥2
    // amplifiers, so the amplifier↔detail bijection is exercised non-vacuously.
    let proj = home.path().join("Dropbox/proj");
    fs::create_dir_all(proj.join(".git")).unwrap();
    fs::create_dir_all(proj.join("sub")).unwrap();
    fs::write(proj.join("sub/a.jsonl"), "x\ny\n").unwrap();
    // A second, plain artifact to also cover findings with few/no amplifiers.
    let plain = home.path().join("plain");
    fs::create_dir_all(&plain).unwrap();
    fs::write(plain.join("b.jsonl"), "z\n").unwrap();

    let cfg = ScanConfig {
        extra_definitions: vec![
            common::sig("amp", "~/Dropbox/**/*.jsonl", "file"),
            common::sig("plain", "~/plain/**/*.jsonl", "file"),
        ],
        no_slow: true,
        ..ScanConfig::for_home(home.path())
    };
    let res = scan(&cfg);
    assert!(!res.findings.is_empty(), "expected findings");

    let mut saw_amplified = false;
    for f in &res.findings {
        // (1) Every finding carries a non-empty rationale.
        assert!(
            !f.why.trim().is_empty(),
            "finding {} has an empty why",
            f.definition_id
        );

        // (2) The exposure level recomputes from the emitted axes — it is not opaque.
        assert_eq!(
            f.exposure_level,
            score(f.sensitivity, &f.amplifiers),
            "exposure_level for {} is not reproducible from its axes",
            f.definition_id
        );

        // (3) Amplifier ↔ amplifier_detail is a bijection: exactly one detail key per
        //     fired amplifier, and no orphan keys.
        let detail = f
            .amplifier_detail
            .as_object()
            .expect("amplifier_detail is a JSON object");
        assert_eq!(
            detail.len(),
            f.amplifiers.len(),
            "amplifier_detail has {} keys but {} amplifiers fired for {}",
            detail.len(),
            f.amplifiers.len(),
            f.definition_id
        );
        for a in &f.amplifiers {
            assert!(
                detail.contains_key(a.as_str()),
                "no detail for amplifier {} on {}",
                a.as_str(),
                f.definition_id
            );
        }
        saw_amplified |= !f.amplifiers.is_empty();
    }
    assert!(
        saw_amplified,
        "expected at least one finding with amplifiers (bijection is vacuous otherwise)"
    );
}
