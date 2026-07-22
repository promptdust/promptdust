//! Amplifier detection: given a matched artifact, determine what makes it *more*
//! exposed than a bare plaintext file at rest. This is the differentiating layer.
//!
//! [`detect`] is a pure function of the filesystem plus a few precomputed system
//! facts (disk encryption, backup exclusion), so it is fully testable with real
//! fixtures and plain enum inputs — no mocks.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::platform::{CloudRoot, DiskEncryption, Tri};

/// A condition that increases an artifact's exposure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Amplifier {
    /// The artifact is inside a cloud-synced folder.
    CloudSync,
    /// The artifact is not excluded from the system backup.
    BackupSwept,
    /// The artifact is inside a git working tree.
    InGitRepo,
    /// The artifact is readable by other local users.
    WorldReadable,
    /// Full-disk encryption is off.
    UnencryptedDisk,
    /// The artifact is unusually large (informational).
    LargeGrowth,
}

impl Amplifier {
    /// A stable snake_case identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CloudSync => "cloud_sync",
            Self::BackupSwept => "backup_swept",
            Self::InGitRepo => "in_git_repo",
            Self::WorldReadable => "world_readable",
            Self::UnencryptedDisk => "unencrypted_disk",
            Self::LargeGrowth => "large_growth",
        }
    }
}

/// Precomputed inputs to [`detect`] that are not derivable from the path alone.
pub struct AmpInputs<'a> {
    /// Known cloud-sync roots (computed once per scan).
    pub cloud_roots: &'a [CloudRoot],
    /// Full-disk encryption status (computed once per scan).
    pub disk_encryption: DiskEncryption,
    /// Whether this specific path is excluded from backup (`Unknown` if `no_slow`).
    pub backup_excluded: Tri,
    /// Size threshold above which `LargeGrowth` fires.
    pub large_threshold: u64,
}

/// Detect all amplifiers for `path` (of `size` bytes). Returns the fired amplifiers
/// and a JSON detail object keyed by amplifier name.
#[must_use]
pub fn detect(path: &Path, size: u64, inp: &AmpInputs) -> (Vec<Amplifier>, Value) {
    let mut amps = Vec::new();
    let mut detail = Map::new();

    if let Some(root) = inp.cloud_roots.iter().find(|r| path.starts_with(&r.path)) {
        amps.push(Amplifier::CloudSync);
        detail.insert(
            Amplifier::CloudSync.as_str().to_string(),
            json!({ "provider": root.provider }),
        );
    }

    if let Some(repo_root) = find_git_root(path) {
        amps.push(Amplifier::InGitRepo);
        detail.insert(
            Amplifier::InGitRepo.as_str().to_string(),
            json!({ "repo_root": repo_root.to_string_lossy() }),
        );
    }

    if let Some(mode) = world_readable_mode(path) {
        amps.push(Amplifier::WorldReadable);
        detail.insert(
            Amplifier::WorldReadable.as_str().to_string(),
            json!({ "mode": format!("{mode:04o}") }),
        );
    }

    if inp.backup_excluded == Tri::No {
        amps.push(Amplifier::BackupSwept);
        detail.insert(
            Amplifier::BackupSwept.as_str().to_string(),
            json!({ "excluded": false }),
        );
    }

    if inp.disk_encryption == DiskEncryption::Off {
        amps.push(Amplifier::UnencryptedDisk);
        detail.insert(
            Amplifier::UnencryptedDisk.as_str().to_string(),
            json!({ "status": "off" }),
        );
    }

    if size >= inp.large_threshold {
        amps.push(Amplifier::LargeGrowth);
        detail.insert(
            Amplifier::LargeGrowth.as_str().to_string(),
            json!({ "size_bytes": size, "threshold": inp.large_threshold }),
        );
    }

    (amps, Value::Object(detail))
}

/// Walk up from `path` to find the root of an enclosing git working tree, if any.
fn find_git_root(path: &Path) -> Option<PathBuf> {
    for ancestor in path.ancestors() {
        // `.git` is a directory in a normal clone and a file in a worktree/submodule.
        if ancestor.join(".git").exists() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

/// On Unix, the POSIX permission bits if the file is group- or other-readable.
#[cfg(unix)]
fn world_readable_mode(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::symlink_metadata(path).ok()?;
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o044 != 0 {
        Some(mode)
    } else {
        None
    }
}

/// On non-Unix platforms, ACL-based detection is deferred to M7; report nothing
/// rather than a false negative disguised as a positive.
#[cfg(not(unix))]
fn world_readable_mode(_path: &Path) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs<'a>(roots: &'a [CloudRoot], disk: DiskEncryption, backup: Tri) -> AmpInputs<'a> {
        AmpInputs {
            cloud_roots: roots,
            disk_encryption: disk,
            backup_excluded: backup,
            large_threshold: 250 * 1024 * 1024,
        }
    }

    #[test]
    fn cloud_sync_fires_under_a_root() {
        let home = tempfile::tempdir().unwrap();
        let dropbox = home.path().join("Dropbox");
        std::fs::create_dir_all(dropbox.join("data")).unwrap();
        let file = dropbox.join("data/a.jsonl");
        std::fs::write(&file, "x").unwrap();
        let roots = vec![CloudRoot {
            provider: "Dropbox".into(),
            path: dropbox,
        }];
        let (amps, detail) = detect(&file, 1, &inputs(&roots, DiskEncryption::On, Tri::Yes));
        assert!(amps.contains(&Amplifier::CloudSync));
        assert_eq!(detail["cloud_sync"]["provider"], "Dropbox");
    }

    #[test]
    fn git_repo_fires_and_reports_root() {
        let home = tempfile::tempdir().unwrap();
        let repo = home.path().join("proj");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("sub")).unwrap();
        let file = repo.join("sub/a.jsonl");
        std::fs::write(&file, "x").unwrap();
        let (amps, detail) = detect(&file, 1, &inputs(&[], DiskEncryption::On, Tri::Yes));
        assert!(amps.contains(&Amplifier::InGitRepo));
        assert!(detail["in_git_repo"]["repo_root"]
            .as_str()
            .unwrap()
            .ends_with("proj"));
    }

    #[cfg(unix)]
    #[test]
    fn world_readable_reflects_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let open = home.path().join("open.jsonl");
        std::fs::write(&open, "x").unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o644)).unwrap();
        let (amps, _) = detect(&open, 1, &inputs(&[], DiskEncryption::On, Tri::Yes));
        assert!(amps.contains(&Amplifier::WorldReadable));

        let shut = home.path().join("shut.jsonl");
        std::fs::write(&shut, "x").unwrap();
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (amps2, _) = detect(&shut, 1, &inputs(&[], DiskEncryption::On, Tri::Yes));
        assert!(!amps2.contains(&Amplifier::WorldReadable));
    }

    #[test]
    fn backup_and_disk_and_size_amplifiers() {
        let home = tempfile::tempdir().unwrap();
        let f = home.path().join("f.jsonl");
        std::fs::write(&f, "x").unwrap();

        // Not excluded from backup + unencrypted disk + over threshold.
        let inp = AmpInputs {
            cloud_roots: &[],
            disk_encryption: DiskEncryption::Off,
            backup_excluded: Tri::No,
            large_threshold: 0,
        };
        let (amps, _) = detect(&f, 1, &inp);
        assert!(amps.contains(&Amplifier::BackupSwept));
        assert!(amps.contains(&Amplifier::UnencryptedDisk));
        assert!(amps.contains(&Amplifier::LargeGrowth));

        // Backup unknown / disk on / under threshold → none of the three.
        let inp2 = AmpInputs {
            cloud_roots: &[],
            disk_encryption: DiskEncryption::On,
            backup_excluded: Tri::Unknown,
            large_threshold: u64::MAX,
        };
        let (amps2, _) = detect(&f, 1, &inp2);
        assert!(!amps2.contains(&Amplifier::BackupSwept));
        assert!(!amps2.contains(&Amplifier::UnencryptedDisk));
        assert!(!amps2.contains(&Amplifier::LargeGrowth));
    }
}
