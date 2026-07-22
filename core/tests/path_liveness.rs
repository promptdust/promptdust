//! Path-liveness gate: every bundled definition's every path applicable to the current OS must
//! be one the scan engine can actually MATCH — a fixture is synthesized *from the pattern
//! itself* and the definition must fire on it. This catches patterns the engine can't match
//! against their own fixture: malformed globs, char-class/brace metacharacters the matcher
//! reads differently than an author intends (e.g. `[A]` as a class, not a literal), and
//! matcher regressions. It does **not** verify a path points at a real-world store — a
//! well-formed but wrong path (a typo) still passes; that is what the `verified` tier + a
//! real-install check are for. Across the CI matrix each per-OS path is exercised on its own
//! OS (on PR runs — the cross-OS legs don't run on direct pushes to main).
//!
//! `catalog.rs` is the narrower promise (`verified`-tier sigs, first path only). This is the
//! matchability check over every tier and every path.

mod common;

use promptdust_core::{scan, MatchKind, Platform, ScanConfig};
use std::fs;

#[test]
fn every_bundled_path_matches_a_synthesized_fixture() {
    let Some(plat) = Platform::current() else {
        return;
    };
    let loaded = promptdust_core::definitions::load_bundled();

    let mut checked = 0;
    for sig in loaded.definitions.iter().filter(|s| s.applies_to(plat)) {
        // A sig that declares this platform must have at least one path for it — otherwise it
        // silently contributes nothing (the global `checked > 0` below can't catch that).
        let paths: Vec<_> = sig.applicable_paths(plat).collect();
        assert!(
            !paths.is_empty(),
            "definition '{}' declares platform {plat:?} but has no path applicable to it",
            sig.id
        );
        for path in paths {
            let home = tempfile::tempdir().unwrap();
            let target = common::synthesize(home.path(), &path.pattern);
            match path.match_kind {
                MatchKind::Dir => {
                    fs::create_dir_all(&target).unwrap();
                    fs::write(target.join("inner.bin"), b"x").unwrap();
                }
                _ => {
                    fs::create_dir_all(target.parent().unwrap()).unwrap();
                    fs::write(&target, b"x\n").unwrap();
                }
            }

            let cfg = ScanConfig {
                only: vec![sig.id.clone()],
                no_slow: true,
                ..ScanConfig::for_home(home.path())
            };
            let res = scan(&cfg);
            assert!(
                res.findings.iter().any(|f| f.definition_id == sig.id),
                "definition '{}' did not match a fixture synthesized at its own path {} (pattern {})",
                sig.id,
                target.display(),
                path.pattern
            );
            checked += 1;
        }
    }

    assert!(
        checked > 0,
        "expected at least one bundled path applicable to this OS"
    );
}
