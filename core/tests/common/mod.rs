//! Shared helpers for integration tests. All fixtures are synthetic; no real data.
#![allow(dead_code)] // not every test binary uses every helper

use promptdust_core::{scan, Definition, ScanConfig, ScanResult};
use std::path::{Path, PathBuf};

/// The OS string for the platform the tests are compiled for.
#[must_use]
pub fn os_str() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    }
}

/// Build a definition for the current OS from a pattern and match kind.
#[must_use]
pub fn sig(id: &str, pattern: &str, match_kind: &str) -> Definition {
    let json = format!(
        r#"{{
            "schema_version": 1, "id": "{id}", "tool": "Test {id}",
            "platforms": ["{os}"],
            "paths": [{{ "pattern": "{pattern}", "match": "{match_kind}" }}],
            "category": "transcript", "format": "jsonl", "sensitivity": "high",
            "why": "synthetic test artifact"
        }}"#,
        os = os_str(),
    );
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("bad test definition json: {e}"))
}

/// Like [`sig`] but with a metadata-only inspector attached.
#[must_use]
pub fn sig_inspector(id: &str, pattern: &str, match_kind: &str, inspector: &str) -> Definition {
    let json = format!(
        r#"{{
            "schema_version": 1, "id": "{id}", "tool": "Test {id}",
            "platforms": ["{os}"],
            "paths": [{{ "pattern": "{pattern}", "match": "{match_kind}" }}],
            "category": "transcript", "format": "jsonl", "sensitivity": "high",
            "inspector": "{inspector}", "why": "synthetic test artifact"
        }}"#,
        os = os_str(),
    );
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("bad test definition json: {e}"))
}

/// Turn a definition `pattern` into one concrete matching path under `home`:
/// `**` → a directory segment, `*globs*` → a filename, literals kept as-is.
#[must_use]
pub fn synthesize(home: &Path, pattern: &str) -> PathBuf {
    let rest = pattern.strip_prefix("~/").unwrap_or(pattern);
    let mut p = home.to_path_buf();
    for comp in rest.split('/') {
        let seg = if comp == "**" {
            "sampledir".to_string()
        } else if comp.contains('*') {
            comp.replace('*', "sample")
        } else {
            comp.to_string()
        };
        if !seg.is_empty() {
            p.push(seg);
        }
    }
    p
}

/// Scan `home` with only the given definition (isolated from the bundled catalog).
/// Slow shell-out probes are skipped for speed/determinism; amplifier integration
/// tests opt back in explicitly.
#[must_use]
pub fn scan_only(home: &Path, definition: Definition) -> ScanResult {
    let cfg = ScanConfig {
        only: vec![definition.id.clone()],
        extra_definitions: vec![definition],
        no_slow: true,
        ..ScanConfig::for_home(home)
    };
    scan(&cfg)
}
