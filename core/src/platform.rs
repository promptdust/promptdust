//! OS integration: directory discovery plus the system-state probes that the
//! amplifier detectors depend on (Time Machine exclusion, disk encryption, cloud
//! roots).
//!
//! Every probe is read-only and degrades to `Unknown` when the underlying tool is
//! missing or errors — never a crash, never a silent false negative (AC-4.4).
//!
//! `PROMPTDUST_HOME` / `PROMPTDUST_DEFINITIONS_DIR` env overrides exist for local
//! testing so a scan can target a synthetic fixture tree instead of the real HOME.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

/// The user's home directory, honoring the `PROMPTDUST_HOME` test override.
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    if let Ok(h) = env::var("PROMPTDUST_HOME") {
        if !h.is_empty() {
            return Some(PathBuf::from(h));
        }
    }
    dirs::home_dir()
}

/// The directory where user-supplied definition files are merged from, honoring the
/// `PROMPTDUST_DEFINITIONS_DIR` test override.
#[must_use]
pub fn user_definitions_dir() -> Option<PathBuf> {
    if let Ok(d) = env::var("PROMPTDUST_DEFINITIONS_DIR") {
        if !d.is_empty() {
            return Some(PathBuf::from(d));
        }
    }
    dirs::config_dir().map(|c| c.join("promptdust").join("definitions"))
}

/// A three-valued answer for probes that can be indeterminate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tri {
    /// Definitely yes.
    Yes,
    /// Definitely no.
    No,
    /// Could not be determined.
    Unknown,
}

/// Full-disk encryption status of the volume holding the user's data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiskEncryption {
    /// Full-disk encryption is on.
    On,
    /// Full-disk encryption is off.
    Off,
    /// Could not be determined.
    Unknown,
}

/// A cloud-sync root directory and the provider it belongs to.
#[derive(Debug, Clone)]
pub struct CloudRoot {
    /// Provider name (e.g. `iCloud`, `Dropbox`, `OneDrive`).
    pub provider: String,
    /// The root path under which files are synced.
    pub path: PathBuf,
}

/// Read-only OS system-state probes used by the amplifier detectors.
pub trait SystemProbe: Send + Sync {
    /// Directories under which files are synced to a cloud provider.
    fn cloud_sync_roots(&self) -> Vec<CloudRoot>;
    /// Whether `path` is excluded from the system backup (`Yes` = excluded/safe).
    fn is_backup_excluded(&self, path: &Path) -> Tri;
    /// Full-disk encryption status.
    fn disk_encryption(&self) -> DiskEncryption;
}

/// Construct the probe for the current OS, rooted at `home`.
#[must_use]
pub fn system_probe(home: &Path) -> Box<dyn SystemProbe> {
    let home = home.to_path_buf();
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacProbe { home })
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxProbe { home })
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsProbe { home })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Box::new(FallbackProbe { home })
    }
}

/// Run a command with a timeout, returning its stdout on success. Any failure
/// (missing binary, non-zero exit, timeout) yields `None`.
fn run(cmd: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .ok()?;

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    }
    let out = child.wait_with_output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Cloud roots discoverable purely from the filesystem layout, shared by all OSes.
fn common_cloud_roots(home: &Path) -> Vec<CloudRoot> {
    let mut roots = Vec::new();

    // macOS iCloud Drive.
    let icloud = home.join("Library/Mobile Documents");
    if icloud.is_dir() {
        roots.push(CloudRoot {
            provider: "iCloud".to_string(),
            path: icloud,
        });
    }

    // macOS FileProvider mounts: ~/Library/CloudStorage/<Provider>-<account>
    let cloud_storage = home.join("Library/CloudStorage");
    if let Ok(entries) = std::fs::read_dir(&cloud_storage) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let provider = name.split('-').next().unwrap_or(&name).to_string();
            roots.push(CloudRoot {
                provider,
                path: entry.path(),
            });
        }
    }

    // Legacy / cross-platform home-level roots.
    for (provider, sub) in [("Dropbox", "Dropbox"), ("OneDrive", "OneDrive")] {
        let p = home.join(sub);
        if p.is_dir() {
            roots.push(CloudRoot {
                provider: provider.to_string(),
                path: p,
            });
        }
    }

    roots
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{
        common_cloud_roots, run, CloudRoot, DiskEncryption, SystemProbe, Tri, PROBE_TIMEOUT,
    };
    use std::path::{Path, PathBuf};

    pub struct MacProbe {
        pub home: PathBuf,
    }

    impl SystemProbe for MacProbe {
        fn cloud_sync_roots(&self) -> Vec<CloudRoot> {
            common_cloud_roots(&self.home)
        }

        fn is_backup_excluded(&self, path: &Path) -> Tri {
            let Some(p) = path.to_str() else {
                return Tri::Unknown;
            };
            match run("tmutil", &["isexcluded", p], PROBE_TIMEOUT) {
                Some(out) if out.contains("[Excluded]") => Tri::Yes,
                Some(out) if out.contains("[Included]") => Tri::No,
                _ => Tri::Unknown,
            }
        }

        fn disk_encryption(&self) -> DiskEncryption {
            match run("fdesetup", &["status"], PROBE_TIMEOUT) {
                Some(out) if out.contains("FileVault is On") => DiskEncryption::On,
                Some(out) if out.contains("FileVault is Off") => DiskEncryption::Off,
                _ => DiskEncryption::Unknown,
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{
        common_cloud_roots, run, CloudRoot, DiskEncryption, SystemProbe, Tri, PROBE_TIMEOUT,
    };
    use std::path::{Path, PathBuf};

    pub struct LinuxProbe {
        pub home: PathBuf,
    }

    impl SystemProbe for LinuxProbe {
        fn cloud_sync_roots(&self) -> Vec<CloudRoot> {
            common_cloud_roots(&self.home)
        }

        // Linux has no single standard system backup analogous to Time Machine.
        fn is_backup_excluded(&self, _path: &Path) -> Tri {
            Tri::Unknown
        }

        fn disk_encryption(&self) -> DiskEncryption {
            // Best-effort: a block device of type `crypt` implies LUKS/dm-crypt is in
            // use. This is a heuristic, not proof the user's data volume is encrypted.
            match run("lsblk", &["-o", "TYPE", "--noheadings"], PROBE_TIMEOUT) {
                Some(out) if out.lines().any(|l| l.trim() == "crypt") => DiskEncryption::On,
                Some(_) => DiskEncryption::Off,
                None => DiskEncryption::Unknown,
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::{
        common_cloud_roots, run, CloudRoot, DiskEncryption, SystemProbe, Tri, PROBE_TIMEOUT,
    };
    use std::path::{Path, PathBuf};

    pub struct WindowsProbe {
        pub home: PathBuf,
    }

    impl SystemProbe for WindowsProbe {
        fn cloud_sync_roots(&self) -> Vec<CloudRoot> {
            let mut roots = common_cloud_roots(&self.home);
            for var in ["OneDrive", "OneDriveCommercial", "OneDriveConsumer"] {
                if let Ok(v) = std::env::var(var) {
                    if !v.is_empty() {
                        roots.push(CloudRoot {
                            provider: "OneDrive".to_string(),
                            path: PathBuf::from(v),
                        });
                    }
                }
            }
            roots
        }

        // No standard, queryable per-file backup-exclusion on Windows yet.
        fn is_backup_excluded(&self, _path: &Path) -> Tri {
            Tri::Unknown
        }

        fn disk_encryption(&self) -> DiskEncryption {
            match run("manage-bde", &["-status", "C:"], PROBE_TIMEOUT) {
                Some(out) if out.contains("Protection On") => DiskEncryption::On,
                Some(out) if out.contains("Protection Off") => DiskEncryption::Off,
                _ => DiskEncryption::Unknown,
            }
        }
    }
}

/// Fallback probe for any other OS: reports `Unknown` rather than a false answer.
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
struct FallbackProbe {
    home: PathBuf,
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
impl SystemProbe for FallbackProbe {
    fn cloud_sync_roots(&self) -> Vec<CloudRoot> {
        common_cloud_roots(&self.home)
    }
    fn is_backup_excluded(&self, _path: &Path) -> Tri {
        Tri::Unknown
    }
    fn disk_encryption(&self) -> DiskEncryption {
        DiskEncryption::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_binary_yields_none() {
        assert!(run("this-binary-does-not-exist-xyz", &[], PROBE_TIMEOUT).is_none());
    }

    #[test]
    fn common_cloud_roots_detects_dropbox_fixture() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join("Dropbox")).unwrap();
        let roots = common_cloud_roots(home.path());
        assert!(roots.iter().any(|r| r.provider == "Dropbox"));
    }

    #[test]
    fn common_cloud_roots_detects_icloud_and_cloudstorage() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join("Library/Mobile Documents")).unwrap();
        std::fs::create_dir_all(home.path().join("Library/CloudStorage/OneDrive-Personal"))
            .unwrap();
        let roots = common_cloud_roots(home.path());
        assert!(roots.iter().any(|r| r.provider == "iCloud"));
        assert!(roots.iter().any(|r| r.provider == "OneDrive"));
    }

    #[cfg(unix)]
    #[test]
    fn run_captures_stdout_of_a_quick_command() {
        assert_eq!(
            run("printf", &["hello"], PROBE_TIMEOUT).as_deref(),
            Some("hello")
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_times_out_and_kills_a_slow_command() {
        // `sleep 5` far exceeds the tiny timeout → killed → None (no hang).
        assert!(run("sleep", &["5"], std::time::Duration::from_millis(80)).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn run_returns_none_on_nonzero_exit() {
        assert!(run("false", &[], PROBE_TIMEOUT).is_none());
    }

    #[test]
    fn disk_encryption_probe_is_always_defined() {
        // On every OS the real probe (fdesetup / lsblk / manage-bde) must return a
        // defined value and never panic. On unsupported OSes it degrades to Unknown.
        let probe = system_probe(Path::new("/"));
        assert!(matches!(
            probe.disk_encryption(),
            DiskEncryption::On | DiskEncryption::Off | DiskEncryption::Unknown
        ));
    }

    #[test]
    fn backup_probe_is_always_defined() {
        let probe = system_probe(Path::new("/tmp"));
        assert!(matches!(
            probe.is_backup_excluded(Path::new("/tmp")),
            Tri::Yes | Tri::No | Tri::Unknown
        ));
    }
}
