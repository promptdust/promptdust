//! Performance baseline (NFR-1). Marked `#[ignore]` so it is not a flaky CI gate;
//! run it deliberately with `cargo test -p promptdust-core --test perf -- --ignored`.

mod common;

use common::sig;
use promptdust_core::{scan, ScanConfig};
use std::fs;
use std::time::Instant;

#[test]
#[ignore = "performance baseline; run with --ignored"]
fn scan_of_a_large_tree_completes_quickly() {
    let home = tempfile::tempdir().unwrap();
    let base = home.path().join("data");

    // 10,000 matching files across 100 directories.
    for d in 0..100 {
        let dir = base.join(format!("d{d}"));
        fs::create_dir_all(&dir).unwrap();
        for f in 0..100 {
            fs::write(dir.join(format!("f{f}.jsonl")), "x\n").unwrap();
        }
    }

    let cfg = ScanConfig {
        only: vec!["big".to_string()],
        extra_definitions: vec![sig("big", "~/data/**/*.jsonl", "file")],
        no_slow: true,
        ..ScanConfig::for_home(home.path())
    };

    let started = Instant::now();
    let res = scan(&cfg);
    let elapsed = started.elapsed();

    eprintln!(
        "PERF: scanned {} files (incl. amplifier detection) in {elapsed:?}",
        res.findings.len()
    );
    assert_eq!(res.findings.len(), 10_000);
    // A generous ceiling so this is a regression tripwire, not a flaky micro-benchmark.
    assert!(elapsed.as_secs() < 30, "scan far too slow: {elapsed:?}");
}
