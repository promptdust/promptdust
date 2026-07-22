//! End-to-end amplifier detection through a full scan, using real fixtures and (on
//! macOS) the real Time Machine / FileVault probes — no mocks.

mod common;

use promptdust_core::{scan, Amplifier, ExposureLevel, ScanConfig};
use std::fs;

#[test]
fn cloud_sync_and_git_amplifiers_are_detected() {
    let home = tempfile::tempdir().unwrap();
    // A transcript inside BOTH a cloud-synced folder and a git working tree.
    let proj = home.path().join("Dropbox/proj");
    fs::create_dir_all(proj.join(".git")).unwrap();
    fs::create_dir_all(proj.join("sub")).unwrap();
    fs::write(proj.join("sub/a.jsonl"), "x\n").unwrap();

    let cfg = ScanConfig {
        only: vec!["amp".to_string()],
        extra_definitions: vec![common::sig("amp", "~/Dropbox/**/*.jsonl", "file")],
        no_slow: true,
        ..ScanConfig::for_home(home.path())
    };
    let res = scan(&cfg);

    assert_eq!(res.findings.len(), 1);
    let amps = &res.findings[0].amplifiers;
    assert!(amps.contains(&Amplifier::CloudSync), "expected cloud_sync");
    assert!(amps.contains(&Amplifier::InGitRepo), "expected in_git_repo");

    // high(3) + cloud(2) + git(2) [+ world(1) on unix] → critical either way.
    assert_eq!(res.findings[0].exposure_level, ExposureLevel::Critical);

    // Detail object carries the specifics.
    let detail = &res.findings[0].amplifier_detail;
    assert_eq!(detail["cloud_sync"]["provider"], "Dropbox");
    assert!(detail["in_git_repo"]["repo_root"]
        .as_str()
        .unwrap()
        .ends_with("proj"));
}

#[cfg(target_os = "macos")]
#[test]
fn real_macos_probes_run_through_a_full_scan() {
    use promptdust_core::DiskEncryption;

    // Exercises real `fdesetup` and `tmutil` via the scan path (no_slow = false).
    let home = tempfile::tempdir().unwrap();
    let d = home.path().join("d");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("a.jsonl"), "x\n").unwrap();

    let cfg = ScanConfig {
        only: vec!["rp".to_string()],
        extra_definitions: vec![common::sig("rp", "~/d/**/*.jsonl", "file")],
        no_slow: false,
        ..ScanConfig::for_home(home.path())
    };
    let res = scan(&cfg);

    assert_eq!(res.findings.len(), 1);
    assert!(matches!(
        res.disk_encryption,
        DiskEncryption::On | DiskEncryption::Off | DiskEncryption::Unknown
    ));
    assert!(res.findings[0].amplifier_detail.is_object());
}
