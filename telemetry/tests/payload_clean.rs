//! The assembled telemetry payload must leak neither a scanned path nor conversation content
//! — the INV-3 guarantee (#75) extended to the exact bytes the client would send (#78).

use promptdust_core::{scan, ScanConfig};
use promptdust_telemetry::Payload;
use std::fs;

#[test]
fn telemetry_payload_is_path_and_canary_free() {
    const CANARY: &str = "CANARY_SECRET_DO_NOT_LEAK";

    let home = tempfile::tempdir().unwrap();
    // A Claude Code transcript (the bundled definition matches) carrying the canary in content.
    let d = home.path().join(".claude/projects/demo");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("session.jsonl"), format!("{{\"m\":\"{CANARY}\"}}\n")).unwrap();

    let cfg = ScanConfig {
        no_slow: true,
        ..ScanConfig::for_home(home.path())
    };
    let res = scan(&cfg);
    assert!(
        !res.findings.is_empty(),
        "the bundled Claude Code definition should match the fixture"
    );

    let payload = Payload::new(
        &res,
        "9.9.9".to_string(),
        "test".to_string(),
        "test".to_string(),
        Some(1),
        vec!["no_slow".to_string()],
    );
    let json = payload.to_json_pretty();

    assert!(
        !json.contains(CANARY),
        "conversation content leaked into the telemetry payload"
    );
    let hp = home.path().to_string_lossy();
    assert!(
        !json.contains(hp.as_ref()),
        "a scanned path leaked into the telemetry payload"
    );

    // Non-vacuous: it is the telemetry payload, carries the count-only summary + a per-run id,
    // and never the raw findings.
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["kind"], "promptdust-telemetry");
    assert!(v["summary"]["by_definition"].is_object());
    assert_eq!(v["run_id"].as_str().unwrap().len(), 32);
    assert!(
        v.get("findings").is_none(),
        "the telemetry payload must never carry raw findings"
    );
}

#[test]
fn telemetry_doc_documents_every_payload_field() {
    // Doc-drift guard (#79): every key the `Payload` serializes — top-level and inside
    // `summary` — must appear in docs/TELEMETRY.md, so the "documented byte-for-byte" promise
    // can't silently rot when a field is added or renamed.
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/TELEMETRY.md"))
        .expect("docs/TELEMETRY.md should exist");

    // A payload over an empty scan still serializes every field.
    let home = tempfile::tempdir().unwrap();
    let cfg = ScanConfig {
        no_slow: true,
        ..ScanConfig::for_home(home.path())
    };
    let res = scan(&cfg);
    let payload = Payload::new(
        &res,
        "0.0.0".to_string(),
        "os".to_string(),
        "arch".to_string(),
        Some(1),
        Vec::new(),
    );
    let v: serde_json::Value = serde_json::from_str(&payload.to_json_pretty()).unwrap();

    let mut keys: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    keys.extend(v["summary"].as_object().unwrap().keys().cloned());
    // Sanity: the payload really carries the fields we expect to document (non-vacuous).
    assert!(
        keys.contains(&"run_id".to_string()) && keys.contains(&"summary".to_string()),
        "expected the payload's own fields"
    );
    assert!(
        keys.len() >= 15,
        "payload should have many fields, got {}",
        keys.len()
    );

    for k in &keys {
        assert!(
            doc.contains(&format!("\"{k}\"")),
            "docs/TELEMETRY.md must document the `{k}` payload field (drift?)"
        );
    }
}
