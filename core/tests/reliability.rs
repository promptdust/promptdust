//! NFR-2 reliability: a scan never crashes on adverse filesystem conditions. Each
//! hostile fixture must complete (no panic) and surface problems as warnings.

mod common;

use common::{scan_only, sig};
use std::fs;

#[test]
fn unicode_and_emoji_filenames_are_found() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join("store");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("café_🔥_日本.jsonl"), "x\n").unwrap();

    let res = scan_only(home.path(), sig("u", "~/store/**/*.jsonl", "file"));
    assert_eq!(res.findings.len(), 1);
    assert!(res.findings[0].path.to_string_lossy().contains('🔥'));
}

#[test]
fn deeply_nested_tree_does_not_crash() {
    let home = tempfile::tempdir().unwrap();
    let mut deep = home.path().join("d");
    for i in 0..120 {
        deep = deep.join(format!("lvl{i}"));
    }
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("bottom.jsonl"), "x").unwrap();

    let res = scan_only(home.path(), sig("deep", "~/d/**/*.jsonl", "file"));
    assert_eq!(res.findings.len(), 1);
}

#[cfg(unix)]
#[test]
fn permission_denied_directory_becomes_a_warning() {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().unwrap();
    let denied = home.path().join("base/denied");
    fs::create_dir_all(&denied).unwrap();
    fs::write(denied.join("secret.jsonl"), "x").unwrap();
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)).unwrap();

    let res = scan_only(home.path(), sig("p", "~/base/**/*.jsonl", "file"));

    // Restore perms so the tempdir can be cleaned up.
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o755)).unwrap();

    // The unreadable directory must not crash the scan; it surfaces as a warning and
    // we cannot see the file inside it.
    assert!(
        res.warnings
            .iter()
            .any(|w| w.reason.to_lowercase().contains("permission")
                || w.reason.to_lowercase().contains("denied")),
        "expected a permission warning, got: {:?}",
        res.warnings
    );
    assert!(res.findings.is_empty());
}

#[cfg(unix)]
#[test]
fn broken_symlink_does_not_crash() {
    use std::os::unix::fs::symlink;

    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join("store");
    fs::create_dir_all(&dir).unwrap();
    symlink(dir.join("missing-target"), dir.join("dangling.jsonl")).unwrap();

    // match=file must not treat a dangling symlink as a file finding.
    let res_file = scan_only(home.path(), sig("bs", "~/store/**/*.jsonl", "file"));
    assert!(res_file.findings.is_empty());

    // match=either records the symlink itself (present, size 0), still no crash.
    let res_either = scan_only(home.path(), sig("bs2", "~/store/**/*.jsonl", "either"));
    assert_eq!(res_either.findings.len(), 1);
    assert_eq!(res_either.findings[0].size_bytes, 0);
}

#[cfg(unix)]
#[test]
fn symlink_loop_does_not_hang_or_crash() {
    use std::os::unix::fs::symlink;

    let home = tempfile::tempdir().unwrap();
    let a = home.path().join("a");
    fs::create_dir_all(&a).unwrap();
    // a/loop -> a  (a cycle; must not be followed)
    symlink(&a, a.join("loop")).unwrap();
    fs::write(a.join("real.jsonl"), "x").unwrap();

    let res = scan_only(home.path(), sig("loop", "~/a/**/*.jsonl", "file"));
    // Terminates, finds the one real file, does not recurse the cycle.
    assert_eq!(res.findings.len(), 1);
}

#[test]
fn special_and_missing_bases_are_silent() {
    let home = tempfile::tempdir().unwrap();
    // Base directory does not exist at all.
    let res = scan_only(home.path(), sig("nb", "~/nope/**/*.jsonl", "file"));
    assert!(res.findings.is_empty());
    assert!(res.warnings.is_empty());
}
