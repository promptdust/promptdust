//! Catalog integration: every `verified`-tier bundled definition that applies to the
//! current OS must actually match a synthesized fixture at its declared path. This is
//! the M4 gate that keeps `verified` honest.

mod common;

use promptdust_core::{scan, Confidence, Platform, ScanConfig};
use std::fs;

#[test]
fn verified_definitions_match_synthesized_fixtures() {
    let Some(plat) = Platform::current() else {
        return;
    };
    let loaded = promptdust_core::definitions::load_bundled();

    let mut checked = 0;
    for sig in loaded
        .definitions
        .iter()
        .filter(|s| s.confidence == Some(Confidence::Verified) && s.applies_to(plat))
    {
        let path_pattern = sig
            .applicable_paths(plat)
            .next()
            .unwrap_or_else(|| panic!("{}: no applicable path", sig.id));

        let home = tempfile::tempdir().unwrap();
        let target = common::synthesize(home.path(), &path_pattern.pattern);

        // Create the artifact (a directory for match=dir, else a file).
        match path_pattern.match_kind {
            promptdust_core::MatchKind::Dir => {
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
            "verified definition '{}' did not match its own synthesized fixture at {}",
            sig.id,
            target.display()
        );
        checked += 1;
    }

    assert!(
        checked > 0,
        "expected at least one verified definition to check"
    );
}
