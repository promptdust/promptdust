//! Invariant tests that can run at M1. INV-1 (read-only) is the headline: a full
//! scan must leave the filesystem byte-for-byte unchanged.

mod common;

use common::sig;
use promptdust_core::{
    scan, DiagnosticsDocument, EvidenceClass, Host, OutputDocument, RedactedSummary, ScanConfig,
    StorageEpoch, VersionDetect,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// (relative path) -> (is_dir, len, modified_epoch_secs)
type Snapshot = BTreeMap<String, (bool, u64, Option<u64>)>;

fn snapshot(root: &Path) -> Snapshot {
    fn rec(base: &Path, dir: &Path, map: &mut Snapshot) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            let is_dir = meta.is_dir();
            map.insert(rel, (is_dir, meta.len(), mtime));
            if is_dir && !meta.file_type().is_symlink() {
                rec(base, &path, map);
            }
        }
    }
    let mut map = Snapshot::new();
    rec(root, root, &mut map);
    map
}

fn build_fixture(home: &Path) {
    // A transcript-like file tree.
    let data = home.join("data/project");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("a.jsonl"), "line1\nline2\n").unwrap();
    fs::write(data.join("b.jsonl"), "only\n").unwrap();

    // A directory-shaped store with nested files.
    let store = home.join("store/sub");
    fs::create_dir_all(&store).unwrap();
    fs::write(home.join("store/top.bin"), vec![7u8; 200]).unwrap();
    fs::write(store.join("nested.bin"), vec![9u8; 40]).unwrap();

    // A config file.
    fs::write(home.join(".exampletoolrc"), "key=redacted\n").unwrap();
}

#[test]
fn scan_does_not_modify_the_filesystem() {
    let home = tempfile::tempdir().unwrap();
    build_fixture(home.path());

    let before = snapshot(home.path());

    let cfg = ScanConfig {
        extra_definitions: vec![
            sig("f", "~/data/**/*.jsonl", "file"),
            sig("d", "~/store", "dir"),
        ],
        ..ScanConfig::for_home(home.path())
    };
    let res = scan(&cfg);

    // The scan must have actually done work (so we know INV-1 is meaningfully tested).
    assert!(
        res.findings.len() >= 3,
        "expected the two jsonl files and the store dir, got {:?}",
        res.findings
    );

    let after = snapshot(home.path());
    assert_eq!(
        before, after,
        "INV-1 violated: the scan changed the filesystem"
    );
}

#[test]
fn scan_output_never_contains_conversation_content() {
    // INV-3 (output-side, ADR-017): a canary planted inside artifact *content* must never
    // appear in the *emitted* document, even though inspectors read the bytes to count them.
    const CANARY: &str = "CANARY_SECRET_DO_NOT_LEAK";

    let home = tempfile::tempdir().unwrap();

    // JSONL with the canary in the message body.
    let jdir = home.path().join("j");
    fs::create_dir_all(&jdir).unwrap();
    fs::write(
        jdir.join("a.jsonl"),
        format!("{{\"msg\":\"{CANARY}\"}}\n{{\"msg\":\"{CANARY}\"}}\n"),
    )
    .unwrap();

    // SQLite with the canary in a text column.
    let sdir = home.path().join("s");
    fs::create_dir_all(&sdir).unwrap();
    let dbp = sdir.join("state.vscdb");
    let conn = rusqlite::Connection::open(&dbp).unwrap();
    conn.execute("CREATE TABLE chat (id INTEGER, body TEXT)", [])
        .unwrap();
    conn.execute("INSERT INTO chat VALUES (1, ?1), (2, ?1)", [CANARY])
        .unwrap();
    drop(conn);

    // A v2-enriched definition (version_detect / storage_epochs) over a canary-bearing
    // artifact — the enriched fields must not open a new content-emission path. Those
    // fields have no engine consumer yet, so the `~/v/version` file is a forward tripwire
    // (it guards that a future reader of them won't emit content); today only the `.jsonl`,
    // read by `jsonl_linecount`, actually exercises emission.
    let vdir = home.path().join("v");
    fs::create_dir_all(&vdir).unwrap();
    fs::write(vdir.join("b.jsonl"), format!("{{\"m\":\"{CANARY}\"}}\n")).unwrap();
    fs::write(vdir.join("version"), format!("v={CANARY}")).unwrap();
    let mut v2_sig = common::sig_inspector("v2", "~/v/**/*.jsonl", "file", "jsonl_linecount");
    v2_sig.max_evidence_class = Some(EvidenceClass::Content);
    v2_sig.version_detect = Some(VersionDetect {
        source: "~/v/version".into(),
        kind: None,
    });
    v2_sig.storage_epochs = vec![StorageEpoch {
        label: "e1".into(),
        note: Some("legacy".into()),
    }];

    let cfg = ScanConfig {
        extra_definitions: vec![
            common::sig_inspector("jc", "~/j/**/*.jsonl", "file", "jsonl_linecount"),
            common::sig_inspector("sc", "~/s/**/*.vscdb", "file", "sqlite_rowcount"),
            v2_sig,
        ],
        no_slow: true,
        ..ScanConfig::for_home(home.path())
    };
    let res = scan(&cfg);

    // The inspectors must have actually run (otherwise the test proves nothing).
    assert!(
        res.findings
            .iter()
            .any(|f| f.inspection.as_ref().and_then(|i| i.line_count) == Some(2)),
        "jsonl inspector should have counted 2 lines"
    );
    assert!(
        res.findings
            .iter()
            .any(|f| f.inspection.as_ref().and_then(|i| i.row_count) == Some(2)),
        "sqlite inspector should have counted 2 rows"
    );

    // Assert on the *emitted* OutputDocument (what actually leaves the process), not the
    // internal ScanResult — this also covers the mode/schema headers.
    let doc = OutputDocument::new(
        &res,
        "2026-01-01T00:00:00Z".to_string(),
        Host {
            os: "test".into(),
            arch: "test".into(),
            os_version: None,
        },
    );
    let json = doc.to_json_pretty();
    assert!(
        !json.contains(CANARY),
        "INV-3 violated: artifact content leaked into the emitted document"
    );

    // Every emitted evidence_class is a closed-enum value — no free-text passthrough.
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let findings = v["findings"].as_array().expect("findings array");
    assert!(!findings.is_empty(), "expected findings to check");
    for f in findings {
        let ec = f["evidence_class"].as_str().expect("evidence_class string");
        assert!(
            matches!(ec, "presence" | "usage" | "content"),
            "evidence_class must be a closed-enum value, got {ec:?}"
        );
    }
}

#[test]
fn redacted_summary_is_path_and_canary_free() {
    // `RedactedSummary` (core/src/redact.rs) is the shared, path-scrubbed projection that
    // every diagnostics bundle (#76) and telemetry payload (#78) is built from. A scan's
    // findings carry absolute paths and its inspectors read canary-bearing content, so the
    // projection must leak neither a filesystem path nor that content.
    const CANARY: &str = "CANARY_SECRET_DO_NOT_LEAK";

    let home = tempfile::tempdir().unwrap();

    // Canary in a JSONL body, read by the line-count inspector.
    let jdir = home.path().join("j");
    fs::create_dir_all(&jdir).unwrap();
    fs::write(
        jdir.join("a.jsonl"),
        format!("{{\"msg\":\"{CANARY}\"}}\n{{\"msg\":\"{CANARY}\"}}\n"),
    )
    .unwrap();

    // A second, directory-shaped store so `by_definition` has more than one entry.
    let sdir = home.path().join("store/sub");
    fs::create_dir_all(&sdir).unwrap();
    fs::write(sdir.join("nested.bin"), vec![9u8; 40]).unwrap();

    let cfg = ScanConfig {
        extra_definitions: vec![
            common::sig_inspector("jc", "~/j/**/*.jsonl", "file", "jsonl_linecount"),
            common::sig("st", "~/store", "dir"),
        ],
        no_slow: true,
        ..ScanConfig::for_home(home.path())
    };
    let res = scan(&cfg);

    // The scan must have produced a finding from each definition (else the test is vacuous).
    assert!(
        res.findings.iter().any(|f| f.definition_id == "jc"),
        "expected the jsonl finding"
    );
    assert!(
        res.findings.iter().any(|f| f.definition_id == "st"),
        "expected the store-dir finding"
    );

    let summary = RedactedSummary::from_scan(&res);

    // Behavior: per-definition counts are derived from the findings (not just copied through).
    assert_eq!(summary.by_definition.get("jc"), Some(&1));
    assert_eq!(summary.by_definition.get("st"), Some(&1));
    assert_eq!(summary.total_findings, res.findings.len());

    // The projection is what would leave the machine — serialize it and prove it is clean.
    let json = serde_json::to_string(&summary).unwrap();
    assert!(
        !json.contains(CANARY),
        "INV-3: artifact content leaked into the redacted summary"
    );
    // No resolved path may appear. Every finding resolves under the fixture home, so a JSON
    // free of the home root is necessarily free of every finding's absolute path — and of the
    // username-bearing prefix a real machine would carry.
    let hp = home.path().to_string_lossy();
    assert!(
        !json.contains(hp.as_ref()),
        "the scan home path leaked into the redacted summary"
    );
}

#[test]
fn diagnostics_document_is_path_and_canary_free() {
    // The assembled diagnostics bundle (#76) wraps the RedactedSummary with host/version
    // metadata — it is what a user pastes into a bug report, so it too must leak neither the
    // canary content nor a resolved path.
    const CANARY: &str = "CANARY_SECRET_DO_NOT_LEAK";

    let home = tempfile::tempdir().unwrap();
    let jdir = home.path().join("j");
    fs::create_dir_all(&jdir).unwrap();
    fs::write(jdir.join("a.jsonl"), format!("{{\"msg\":\"{CANARY}\"}}\n")).unwrap();

    let cfg = ScanConfig {
        extra_definitions: vec![common::sig_inspector(
            "jc",
            "~/j/**/*.jsonl",
            "file",
            "jsonl_linecount",
        )],
        no_slow: true,
        ..ScanConfig::for_home(home.path())
    };
    let res = scan(&cfg);
    assert!(
        !res.findings.is_empty(),
        "expected a finding so the test is meaningful"
    );

    let doc = DiagnosticsDocument::new(
        &res,
        "2026-01-01T00:00:00Z".to_string(),
        "0.0.0-test".to_string(),
        Host {
            os: "test".to_string(),
            arch: "test".to_string(),
            os_version: None,
        },
        Some(1),
    );
    let json = doc.to_json_pretty();
    assert!(
        !json.contains(CANARY),
        "INV-3: content leaked into the diagnostics bundle"
    );
    let hp = home.path().to_string_lossy();
    assert!(
        !json.contains(hp.as_ref()),
        "a resolved path leaked into the diagnostics bundle"
    );

    // It is the diagnostics shape and carries the count-only summary (non-vacuous).
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["kind"], "promptdust-diagnostics");
    assert_eq!(v["summary"]["by_definition"]["jc"], 1);
}

#[test]
fn ring_zero_caps_evidence_at_presence() {
    // Consent-ring seam: a definition that declares it can yield `content`, scanned at the
    // default Ring 0 (Inventory), still emits only `presence`-class evidence — the ring's
    // reach caps it. Deeper evidence requires an explicit deeper ring (not built yet).
    let home = tempfile::tempdir().unwrap();
    let jdir = home.path().join("j");
    fs::create_dir_all(&jdir).unwrap();
    fs::write(jdir.join("a.jsonl"), "x\ny\n").unwrap();

    // A definition that declares it can yield content (helper + one field).
    let mut sig = common::sig_inspector("c", "~/j/**/*.jsonl", "file", "jsonl_linecount");
    sig.max_evidence_class = Some(EvidenceClass::Content);

    // Default config → mode is Ring 0 (Inventory).
    let cfg = ScanConfig {
        extra_definitions: vec![sig],
        no_slow: true,
        ..ScanConfig::for_home(home.path())
    };
    let res = scan(&cfg);

    assert_eq!(res.mode.as_str(), "inventory");
    assert_eq!(res.findings.len(), 1);
    // The inspector ran (presence-class metadata) but evidence is capped at presence.
    assert_eq!(
        res.findings[0]
            .inspection
            .as_ref()
            .and_then(|i| i.line_count),
        Some(2)
    );
    assert_eq!(res.findings[0].evidence_class.as_str(), "presence");
}

#[test]
fn scan_creates_no_report_file_by_default() {
    // INV-4 (partial, at core level): the engine writes nothing itself.
    let home = tempfile::tempdir().unwrap();
    build_fixture(home.path());
    let before = snapshot(home.path());
    let _ = scan(&ScanConfig::for_home(home.path()));
    let after = snapshot(home.path());
    assert_eq!(before, after, "the core scan must not write any file");
}
