//! CLI integration tests: the JSON contract, `--json` purity, INV-4 (no unrequested
//! writes), the `--output` sensitivity warning, and the definitions/version commands.
//! Every scan runs against a synthetic fixture HOME via `PROMPTDUST_HOME`.

use assert_cmd::Command;
use predicates::prelude::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use tempfile::{tempdir, TempDir};

/// Build a fixture HOME with one Claude Code transcript, plus an isolated (empty)
/// user-definitions dir so tests never read the real `~/.config`.
fn fixture() -> (TempDir, TempDir) {
    let home = tempdir().unwrap();
    let sigs = tempdir().unwrap();
    let d = home.path().join(".claude/projects/demo");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("session.jsonl"), "a\nb\nc\n").unwrap();
    (home, sigs)
}

fn cmd(home: &Path, sigs: &Path) -> Command {
    let mut c = Command::cargo_bin("promptdust").unwrap();
    c.env("PROMPTDUST_HOME", home)
        // Mirror the production layout: the consent file lives in the config dir and the user
        // definitions in a *child* of it — so a written `consent.json` never lands in the dir
        // the definition loader scans. Clear ambient telemetry env so tests are deterministic.
        .env("PROMPTDUST_CONFIG_DIR", sigs)
        .env("PROMPTDUST_DEFINITIONS_DIR", sigs.join("definitions"))
        .env_remove("DO_NOT_TRACK")
        .env_remove("CI")
        .env_remove("PROMPTDUST_TELEMETRY");
    c
}

fn file_set(root: &Path) -> BTreeSet<String> {
    fn rec(base: &Path, dir: &Path, acc: &mut BTreeSet<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                acc.insert(p.strip_prefix(base).unwrap().to_string_lossy().into_owned());
                if p.is_dir() {
                    rec(base, &p, acc);
                }
            }
        }
    }
    let mut acc = BTreeSet::new();
    rec(root, root, &mut acc);
    acc
}

#[test]
fn json_output_is_pure_and_valid() {
    let (home, sigs) = fixture();
    let out = cmd(home.path(), sigs.path())
        .args(["scan", "--json", "--no-slow"])
        .assert()
        .success()
        .get_output()
        .clone();

    let stdout = String::from_utf8(out.stdout).unwrap();
    // Pure JSON: parses whole, and starts with '{'.
    assert!(stdout.trim_start().starts_with('{'));
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be pure JSON");

    assert_eq!(v["schema_version"], 1);
    assert!(v["generated_at"].as_str().unwrap().contains('T'));
    assert_eq!(v["host"]["os"], std::env::consts::OS);
    // Additive output-contract fields (SCHEMA_VERSION stays 1).
    assert_eq!(v["mode"], "inventory");
    // The endpoint dual score is surfaced (the CLI supplies now_epoch to the clockless core).
    assert!(v["exposure"]["score"].is_number(), "exposure score present");
    assert!(v["assurance"]["band"].is_string(), "assurance band present");
    let interp = v["interpretation"]
        .as_str()
        .expect("interpretation present");
    assert!(
        !interp.contains("clean"),
        "interpretation must not use a 'clean' verdict"
    );
    assert!(v["coverage_gaps"].is_array() && v["corroborations"].is_array());
    let findings = v["findings"].as_array().unwrap();
    assert!(
        findings.iter().all(|f| f["evidence_class"] == "presence"),
        "every finding carries a presence-class evidence label at Ring 0"
    );
    // Per-finding re-derivable exposure factors (Charter A6 explainability).
    assert!(
        findings.iter().all(|f| f["computed"]["p"].is_number()),
        "every finding carries its re-derivable computed factors"
    );
    let ids: Vec<&str> = findings
        .iter()
        .map(|f| f["definition_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"claude-code-transcripts"));
}

#[test]
fn diagnostics_bundle_is_pure_json_and_path_free() {
    // `diagnostics` emits the redacted bug-report bundle: pure JSON on stdout (the review
    // notice goes to stderr), no raw findings, and no scanned path.
    let (home, sigs) = fixture();
    let before = file_set(home.path());
    let out = cmd(home.path(), sigs.path())
        .args(["diagnostics", "--no-slow"])
        .assert()
        .success()
        .get_output()
        .clone();
    let after = file_set(home.path());
    assert_eq!(
        before, after,
        "diagnostics must not write to the scanned tree"
    );

    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.trim_start().starts_with('{'));
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be pure JSON");
    assert_eq!(v["kind"], "promptdust-diagnostics");
    assert_eq!(v["host"]["os"], std::env::consts::OS);
    // Carries the count-only summary and actually derived it (found the fixture transcript);
    // never the raw findings.
    assert_eq!(v["summary"]["by_definition"]["claude-code-transcripts"], 1);
    assert!(
        v.get("findings").is_none(),
        "diagnostics must not carry raw findings"
    );

    // No scanned path or the fixture home (username-bearing prefix) may appear anywhere.
    let home_str = home.path().to_string_lossy();
    assert!(
        !stdout.contains(home_str.as_ref()),
        "home path leaked into the diagnostics bundle"
    );

    // The review notice is on stderr, keeping stdout pure.
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.to_lowercase().contains("review"),
        "a review notice should print to stderr"
    );
}

#[test]
fn human_output_is_an_inventory_not_a_verdict() {
    let (home, sigs) = fixture();
    cmd(home.path(), sigs.path())
        .args(["scan", "--no-slow"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Claude Code"))
        .stdout(predicate::str::contains("why:"))
        .stdout(predicate::str::contains("you are safe").not())
        .stdout(predicate::str::contains("you're secure").not());
}

#[test]
fn no_report_is_written_without_output_flag() {
    // INV-4: a default scan writes nothing to the filesystem.
    let (home, sigs) = fixture();
    let before = file_set(home.path());
    cmd(home.path(), sigs.path())
        .args(["scan", "--no-slow"])
        .assert()
        .success();
    let after = file_set(home.path());
    assert_eq!(before, after, "INV-4: scan must not write any file");
}

#[test]
fn output_flag_writes_file_and_warns() {
    let (home, sigs) = fixture();
    let outdir = tempdir().unwrap();
    let path = outdir.path().join("report.json");

    cmd(home.path(), sigs.path())
        .args(["scan", "--no-slow", "--output"])
        .arg(&path)
        .assert()
        .success()
        .stderr(predicate::str::contains("carefully"));

    assert!(path.exists());
    let content = fs::read_to_string(&path).unwrap();
    serde_json::from_str::<serde_json::Value>(&content).expect("output must be valid JSON");
}

#[test]
fn output_html_is_self_contained() {
    let (home, sigs) = fixture();
    let outdir = tempdir().unwrap();
    let path = outdir.path().join("report.html");
    cmd(home.path(), sigs.path())
        .args(["scan", "--no-slow", "--output"])
        .arg(&path)
        .assert()
        .success();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.starts_with("<!doctype html"));
    assert!(
        !content.contains("http://"),
        "must not reference external assets"
    );
}

#[test]
fn only_filter_restricts_definitions() {
    let (home, sigs) = fixture();
    let out = cmd(home.path(), sigs.path())
        .args(["scan", "--json", "--no-slow", "--only", "cursor"])
        .assert()
        .success()
        .get_output()
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // Fixture has no Cursor data, so restricting to cursor yields no findings.
    assert!(v["findings"].as_array().unwrap().is_empty());
}

#[test]
fn version_reports_db_version() {
    Command::cargo_bin("promptdust")
        .unwrap()
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains("definitions DB"));
}

#[test]
fn definitions_list_shows_bundled() {
    let sigs = tempdir().unwrap();
    Command::cargo_bin("promptdust")
        .unwrap()
        .env("PROMPTDUST_DEFINITIONS_DIR", sigs.path())
        .args(["definitions", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("claude-code-transcripts"));
}

#[test]
fn definitions_list_json_is_a_public_catalog() {
    let sigs = tempdir().unwrap();
    let out = Command::cargo_bin("promptdust")
        .unwrap()
        .env("PROMPTDUST_DEFINITIONS_DIR", sigs.path())
        .args(["definitions", "list", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.trim_start().starts_with('{'),
        "catalog must be pure JSON"
    );
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("catalog must be valid JSON");
    assert!(
        v["db_version"].as_str().is_some(),
        "catalog carries the DB version"
    );
    let sigs_arr = v["definitions"].as_array().expect("definitions array");
    assert!(!sigs_arr.is_empty(), "catalog lists bundled definitions");
    let cc = sigs_arr
        .iter()
        .find(|s| s["id"] == "claude-code-transcripts")
        .expect("claude-code-transcripts present");
    assert_eq!(cc["tool"], "Claude Code");
    assert!(!cc["why"].as_str().unwrap().is_empty());
    // Internal scoring/detection fields must NOT leak into the public catalog.
    for internal in [
        "inspector",
        "inspector_args",
        "base_weight",
        "max_evidence_class",
        "schema_version",
    ] {
        assert!(
            cc.get(internal).is_none(),
            "internal field {internal} must not appear in the catalog"
        );
    }

    // Deterministic order: definitions are sorted by id, so a committed/consumed catalog is
    // stable and diffable.
    let ids: Vec<&str> = sigs_arr.iter().map(|s| s["id"].as_str().unwrap()).collect();
    let mut sorted_ids = ids.clone();
    sorted_ids.sort_unstable();
    assert_eq!(ids, sorted_ids, "definitions[] must be sorted by id");

    // The per-tool rollup: one entry per distinct tool, name-ordered, joining back to
    // definitions[] with full coverage (each definition in exactly one tool).
    let tools = v["tools"].as_array().expect("tools array");
    assert_eq!(
        v["tool_count"].as_u64().unwrap() as usize,
        tools.len(),
        "tool_count matches tools[]"
    );
    let distinct: BTreeSet<&str> = sigs_arr
        .iter()
        .map(|s| s["tool"].as_str().unwrap())
        .collect();
    assert_eq!(tools.len(), distinct.len(), "one rollup per distinct tool");
    let names: Vec<&str> = tools.iter().map(|t| t["tool"].as_str().unwrap()).collect();
    let mut sorted_names = names.clone();
    sorted_names.sort_by(|a, b| {
        a.to_ascii_lowercase()
            .cmp(&b.to_ascii_lowercase())
            .then_with(|| a.cmp(b))
    });
    assert_eq!(
        names, sorted_names,
        "tools[] must be in case-insensitive name order"
    );
    let all_ids: BTreeSet<&str> = ids.iter().copied().collect();
    let mut covered: Vec<&str> = tools
        .iter()
        .flat_map(|t| t["definition_ids"].as_array().unwrap())
        .map(|x| x.as_str().unwrap())
        .collect();
    for id in &covered {
        assert!(
            all_ids.contains(id),
            "definition_id {id} joins to a definition"
        );
    }
    covered.sort_unstable();
    assert_eq!(
        covered, sorted_ids,
        "each definition is in exactly one tool rollup"
    );

    // confidence_counts is a non-lossy distribution: for every tool it sums to the store count.
    for t in tools {
        let counts = t["confidence_counts"]
            .as_object()
            .expect("confidence_counts object");
        let sum: u64 = counts.values().map(|v| v.as_u64().unwrap()).sum();
        assert_eq!(
            sum,
            t["definition_ids"].as_array().unwrap().len() as u64,
            "tool {} confidence_counts must sum to its store count",
            t["tool"]
        );
    }

    // A multi-store tool rolls up all its stores.
    let cc_tool = tools
        .iter()
        .find(|t| t["tool"] == "Claude Code")
        .expect("Claude Code rollup");
    let cc_ids: BTreeSet<&str> = cc_tool["definition_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    assert!(
        cc_ids.contains("claude-code-transcripts") && cc_ids.contains("claude-code-config"),
        "Claude Code rollup nests both of its stores"
    );
    // Its verification distribution keeps the verified transcript visible (not buried behind
    // the likely config a single tier would have shown).
    assert_eq!(
        cc_tool["confidence_counts"],
        serde_json::json!({"verified": 1, "likely": 1, "unverified": 0, "unspecified": 0})
    );

    // The rollup is a strict public projection: no internal or per-store field leaks in.
    for internal in [
        "confidence",
        "sensitivity",
        "paths",
        "base_weight",
        "inspector",
        "why",
    ] {
        assert!(
            cc_tool.get(internal).is_none(),
            "internal/per-store field {internal} must not appear in a tools[] rollup"
        );
    }

    // Vendor is consistent: the bundled DB keeps one vendor per tool, so a rollup's vendor
    // agrees with every one of its definitions' (guards the silent first-by-id tie-break).
    let sig_vendor: std::collections::HashMap<&str, Option<&str>> = sigs_arr
        .iter()
        .map(|s| (s["id"].as_str().unwrap(), s["vendor"].as_str()))
        .collect();
    for t in tools {
        let tool_vendor = t["vendor"].as_str();
        for sid in t["definition_ids"].as_array().unwrap() {
            if let Some(sv) = sig_vendor[sid.as_str().unwrap()] {
                assert_eq!(
                    tool_vendor,
                    Some(sv),
                    "tool {} vendor disagrees with definition {sid}",
                    t["tool"]
                );
            }
        }
    }
}

#[test]
fn definitions_validate_accepts_valid_and_rejects_invalid() {
    let dir = tempdir().unwrap();

    let good = dir.path().join("good.json");
    fs::write(
        &good,
        r#"{"schema_version":1,"id":"x-y","tool":"X","platforms":["macos"],
            "paths":[{"pattern":"~/x","match":"file"}],
            "category":"transcript","format":"jsonl","sensitivity":"high","why":"b"}"#,
    )
    .unwrap();
    Command::cargo_bin("promptdust")
        .unwrap()
        .args(["definitions", "validate"])
        .arg(&good)
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));

    let bad = dir.path().join("bad.json");
    fs::write(&bad, "{ not json").unwrap();
    Command::cargo_bin("promptdust")
        .unwrap()
        .args(["definitions", "validate"])
        .arg(&bad)
        .assert()
        .failure();
}

#[test]
fn telemetry_defaults_off_and_round_trips() {
    let (home, sigs) = fixture();
    let status_is = |want: &str| {
        cmd(home.path(), sigs.path())
            .args(["telemetry", "status"])
            .assert()
            .success()
            .stdout(predicate::str::contains(format!("telemetry: {want}")));
    };
    status_is("disabled"); // default
    cmd(home.path(), sigs.path())
        .args(["telemetry", "enable"])
        .assert()
        .success();
    status_is("enabled");
    cmd(home.path(), sigs.path())
        .args(["telemetry", "disable"])
        .assert()
        .success();
    status_is("disabled");
}

#[test]
fn telemetry_env_forces_off_even_when_enabled() {
    let (home, sigs) = fixture();
    cmd(home.path(), sigs.path())
        .args(["telemetry", "enable"])
        .assert()
        .success();
    // DO_NOT_TRACK forces telemetry off regardless of the stored "enabled" state.
    cmd(home.path(), sigs.path())
        .env("DO_NOT_TRACK", "1")
        .args(["telemetry", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("forced off"));
}

#[test]
fn telemetry_preview_is_pure_json_path_free_and_findings_free() {
    let (home, sigs) = fixture();
    let out = cmd(home.path(), sigs.path())
        .args(["telemetry", "preview", "--no-slow"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.trim_start().starts_with('{'));
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be pure JSON");
    assert_eq!(v["kind"], "promptdust-telemetry");
    assert_eq!(v["run_id"].as_str().unwrap().len(), 32);
    assert!(
        v.get("findings").is_none(),
        "no raw findings in the payload"
    );
    // The fixture's Claude Code transcript was scanned (non-vacuous) but its path does not leak.
    assert_eq!(v["summary"]["by_definition"]["claude-code-transcripts"], 1);
    let home_str = home.path().to_string_lossy();
    assert!(
        !stdout.contains(home_str.as_ref()),
        "the scan home path leaked into the telemetry payload"
    );
}

#[test]
fn telemetry_preview_run_id_differs_across_runs() {
    let (home, sigs) = fixture();
    let run_id = || {
        let out = cmd(home.path(), sigs.path())
            .args(["telemetry", "preview", "--no-slow"])
            .assert()
            .success()
            .get_output()
            .clone();
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        v["run_id"].as_str().unwrap().to_string()
    };
    assert_ne!(
        run_id(),
        run_id(),
        "the per-run id must be fresh, never persisted"
    );
}
