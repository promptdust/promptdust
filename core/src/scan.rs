//! Scan orchestration: resolve definition paths, walk the filesystem (read-only,
//! fault-tolerant), capture metadata, inspect shape, detect amplifiers, score
//! exposure, and assemble a [`ScanResult`].

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use globset::GlobBuilder;
use walkdir::WalkDir;

use crate::detect::{self, AmpInputs};
use crate::inspect;
use crate::model::{Definition, EvidenceClass, MatchKind, Platform};
use crate::platform::{self, CloudRoot, DiskEncryption, SystemProbe, Tri};
use crate::report::{Finding, Mode, ScanResult, ScanWarning, Summary};
use crate::score;
use crate::{definitions, resolve, SCHEMA_VERSION};

/// Default threshold above which a single store is flagged as `large_growth`.
pub const DEFAULT_LARGE_THRESHOLD: u64 = 250 * 1024 * 1024;

/// Inputs to a scan. Construct with [`ScanConfig::for_home`] (tests) or
/// [`ScanConfig::detect`] (real runs).
#[derive(Debug, Clone, Default)]
pub struct ScanConfig {
    /// The home directory that `~` expands to.
    pub home: PathBuf,
    /// Optional directory of user-supplied definition files to merge in.
    pub user_definitions_dir: Option<PathBuf>,
    /// Extra in-memory definitions (used by tests to stay OS-deterministic).
    pub extra_definitions: Vec<Definition>,
    /// If non-empty, only definitions whose id or tool matches are scanned.
    pub only: Vec<String>,
    /// Definitions whose id or tool matches are skipped.
    pub exclude: Vec<String>,
    /// If set, restrict findings to those under this subtree.
    pub root_override: Option<PathBuf>,
    /// Size threshold for the `large_growth` amplifier (0 → use the default).
    pub large_threshold: u64,
    /// Skip slow shell-out probes (Time Machine, disk encryption).
    pub no_slow: bool,
    /// The consent/depth ring to scan at. Default Ring 0 (`Inventory`) — metadata-only.
    /// Deeper rings require explicit opt-in and are not built yet; this field is the seam
    /// that bounds how deep evidence may reach.
    pub mode: Mode,
    /// Front-end-supplied "now" (Unix seconds) used to score the result (recency + the dual
    /// score). Core stays clockless: when `None`, the scan is not scored (exposure/assurance
    /// stay `None`), so tests and non-scoring callers never depend on a wall clock.
    pub now_epoch: Option<i64>,
}

impl ScanConfig {
    /// A config rooted at `home` with sensible defaults (used mostly by tests).
    #[must_use]
    pub fn for_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            large_threshold: DEFAULT_LARGE_THRESHOLD,
            ..Default::default()
        }
    }

    /// Detect a real config from the environment, or `None` if there is no home dir.
    #[must_use]
    pub fn detect() -> Option<Self> {
        let home = platform::home_dir()?;
        Some(Self {
            home,
            user_definitions_dir: platform::user_definitions_dir(),
            large_threshold: DEFAULT_LARGE_THRESHOLD,
            ..Default::default()
        })
    }

    fn selects(&self, sig: &Definition) -> bool {
        let id = sig.id.to_lowercase();
        let tool = sig.tool.to_lowercase();
        let hit = |list: &[String]| {
            list.iter().any(|q| {
                let q = q.to_lowercase();
                q == id || q == tool
            })
        };
        if !self.only.is_empty() && !hit(&self.only) {
            return false;
        }
        !hit(&self.exclude)
    }

    fn effective_threshold(&self) -> u64 {
        if self.large_threshold == 0 {
            DEFAULT_LARGE_THRESHOLD
        } else {
            self.large_threshold
        }
    }
}

/// How a matched path classifies on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileClass {
    File,
    Dir,
    Symlink,
}

impl FileClass {
    fn satisfies(self, kind: MatchKind) -> bool {
        matches!(
            (self, kind),
            (Self::File, MatchKind::File | MatchKind::Either)
                | (Self::Dir, MatchKind::Dir | MatchKind::Either)
                | (Self::Symlink, MatchKind::Either)
        )
    }
}

/// Mutable state threaded through one scan.
struct Run<'a> {
    cfg: &'a ScanConfig,
    probe: Box<dyn SystemProbe>,
    cloud_roots: Vec<CloudRoot>,
    disk_encryption: DiskEncryption,
    large_threshold: u64,
    findings: Vec<Finding>,
    warnings: Vec<ScanWarning>,
    seen: HashSet<(String, PathBuf)>,
}

/// Run a full scan.
#[must_use]
pub fn scan(cfg: &ScanConfig) -> ScanResult {
    let mut loaded = definitions::load_bundled();
    if let Some(dir) = &cfg.user_definitions_dir {
        definitions::load_user_dir(dir, &mut loaded);
    }
    loaded
        .definitions
        .extend(cfg.extra_definitions.iter().cloned());

    let probe = platform::system_probe(&cfg.home);
    let cloud_roots = probe.cloud_sync_roots();
    let disk_encryption = if cfg.no_slow {
        DiskEncryption::Unknown
    } else {
        probe.disk_encryption()
    };

    let mut run = Run {
        cfg,
        probe,
        cloud_roots,
        disk_encryption,
        large_threshold: cfg.effective_threshold(),
        findings: Vec::new(),
        warnings: loaded.warnings,
        seen: HashSet::new(),
    };

    if let Some(plat) = Platform::current() {
        for sig in &loaded.definitions {
            if !cfg.selects(sig) || !sig.applies_to(plat) {
                continue;
            }
            for path in sig.applicable_paths(plat) {
                let expanded = resolve::expand(&path.pattern, &cfg.home);
                run.collect_matches(sig, &expanded, path.match_kind);
            }
        }
    }

    let mut result = ScanResult {
        schema_version: SCHEMA_VERSION,
        definition_db_version: loaded.db_version,
        disk_encryption: run.disk_encryption,
        mode: cfg.mode,
        exposure: None,
        assurance: None,
        interpretation: None,
        findings: run.findings,
        warnings: run.warnings,
        summary: Summary::default(),
    };
    result.recompute_summary();
    // Score the dual number when the front-end supplied "now" (core stays clockless). The
    // absence-rule needs each finding's definition's declared deepest evidence class.
    if let Some(now) = cfg.now_epoch {
        result.score(now, |id| {
            loaded
                .definitions
                .iter()
                .find(|s| s.id == id)
                .and_then(|s| s.max_evidence_class)
        });
    }
    result
}

impl Run<'_> {
    fn collect_matches(&mut self, sig: &Definition, expanded: &str, kind: MatchKind) {
        if resolve::has_glob(expanded) {
            let base = resolve::split_base(expanded);
            if !base.is_dir() {
                return;
            }
            // Match against forward-slash-normalized paths so a single glob works on
            // Windows (backslash separators) as well as macOS/Linux.
            let normalized = expanded.replace('\\', "/");
            let matcher = match GlobBuilder::new(&normalized)
                .literal_separator(true)
                .build()
            {
                Ok(g) => g.compile_matcher(),
                Err(e) => {
                    self.warnings
                        .push(ScanWarning::new_named(&sig.id, format!("bad pattern: {e}")));
                    return;
                }
            };
            for entry in WalkDir::new(&base).follow_links(false) {
                match entry {
                    Ok(e) => {
                        let p = e.path();
                        if matcher.is_match(resolve::to_match_string(p)) {
                            self.try_add(sig, p, kind);
                        }
                    }
                    Err(e) => self.warnings.push(walk_warning(&e)),
                }
            }
        } else {
            self.try_add(sig, Path::new(expanded), kind);
        }
    }

    fn try_add(&mut self, sig: &Definition, path: &Path, kind: MatchKind) {
        if let Some(root) = &self.cfg.root_override {
            if !path.starts_with(root) {
                return;
            }
        }
        let (class, size, count, mtime) = match measure(path) {
            Ok(v) => v,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    self.warnings
                        .push(ScanWarning::new_path(path, e.to_string()));
                }
                return;
            }
        };
        if !class.satisfies(kind) {
            return;
        }
        if !self.seen.insert((sig.id.clone(), path.to_path_buf())) {
            return;
        }

        let inspection = inspect::inspect(sig, path);
        let backup_excluded = if self.cfg.no_slow {
            Tri::Unknown
        } else {
            self.probe.is_backup_excluded(path)
        };
        let (amplifiers, amplifier_detail) = detect::detect(
            path,
            size,
            &AmpInputs {
                cloud_roots: &self.cloud_roots,
                disk_encryption: self.disk_encryption,
                backup_excluded,
                large_threshold: self.large_threshold,
            },
        );
        let exposure_level = score::score(sig.sensitivity, &amplifiers);
        // The inspect call above produces presence-class metadata (counts/shape) — always
        // allowed (reading bytes to count is fine; only *emitting* content is not). The
        // finding's evidence class is the ring's reach (Ring 0 = presence) capped by what
        // the definition says the artifact can yield: an EMISSION cap, so no ring labels
        // evidence deeper than the user opted into. When a content-class inspector lands,
        // also gate its *derivation* by `self.cfg.mode` (don't compute what you won't emit).
        // (A definition that omits `max_evidence_class` is uncapped here — harmless now;
        // every bundled definition declares it.)
        let evidence_class = self
            .cfg
            .mode
            .max_reach()
            .min(sig.max_evidence_class.unwrap_or(EvidenceClass::Content));

        self.findings.push(Finding {
            definition_id: sig.id.clone(),
            tool: sig.tool.clone(),
            category: sig.category,
            format: sig.format,
            path: path.to_path_buf(),
            size_bytes: size,
            file_count: count,
            modified_epoch_secs: mtime,
            inspection,
            amplifiers,
            amplifier_detail,
            sensitivity: sig.sensitivity,
            evidence_class,
            base_weight: sig.base_weight,
            exposure_level,
            computed: None,
            why: sig.why.clone(),
            guidance: sig.guidance.clone(),
            confidence: sig.confidence,
        });
    }
}

fn measure(path: &Path) -> std::io::Result<(FileClass, u64, u64, Option<u64>)> {
    let meta = fs::symlink_metadata(path)?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let ft = meta.file_type();
    if ft.is_symlink() {
        Ok((FileClass::Symlink, 0, 1, mtime))
    } else if ft.is_dir() {
        let (size, count) = measure_dir(path);
        Ok((FileClass::Dir, size, count, mtime))
    } else {
        Ok((FileClass::File, meta.len(), 1, mtime))
    }
}

fn measure_dir(dir: &Path) -> (u64, u64) {
    let mut size = 0u64;
    let mut count = 0u64;
    for entry in WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() {
            if let Ok(m) = entry.metadata() {
                size = size.saturating_add(m.len());
                count += 1;
            }
        }
    }
    (size, count)
}

fn walk_warning(e: &walkdir::Error) -> ScanWarning {
    let reason = e
        .io_error()
        .map_or_else(|| e.to_string(), std::string::ToString::to_string);
    match e.path() {
        Some(p) => ScanWarning::new_path(p, reason),
        None => ScanWarning::new_named("walk", reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Category, Format, PathPattern, Sensitivity};

    fn sig_for_current_os(id: &str, pattern: &str, kind: MatchKind) -> Definition {
        Definition {
            schema_version: 1,
            id: id.to_string(),
            tool: format!("Tool {id}"),
            vendor: None,
            platforms: vec![Platform::current().expect("supported OS")],
            paths: vec![PathPattern {
                os: None,
                pattern: pattern.to_string(),
                match_kind: kind,
            }],
            category: Category::Transcript,
            format: Format::Jsonl,
            sensitivity: Sensitivity::High,
            inspector: None,
            inspector_args: None,
            retention_hint: None,
            why: "test".to_string(),
            guidance: vec![],
            confidence: None,
            references: vec![],
            ..Default::default()
        }
    }

    fn scan_with(home: &Path, sig: Definition) -> ScanResult {
        // Restrict to this test definition and skip slow probes for speed/determinism.
        let cfg = ScanConfig {
            only: vec![sig.id.clone()],
            extra_definitions: vec![sig],
            no_slow: true,
            ..ScanConfig::for_home(home)
        };
        scan(&cfg)
    }

    #[test]
    fn finds_a_globbed_file() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".claude/projects/foo");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("abc.jsonl"), "a\nb\nc\n").unwrap();
        fs::write(dir.join("ignore.txt"), "x").unwrap();

        let sig = sig_for_current_os("t", "~/.claude/projects/**/*.jsonl", MatchKind::File);
        let res = scan_with(home.path(), sig);

        assert_eq!(res.findings.len(), 1);
        assert!(res.findings[0].path.ends_with("abc.jsonl"));
        assert_eq!(res.findings[0].size_bytes, 6);
        assert_eq!(res.summary.total_findings, 1);
        // Ring-0 output-contract additions: presence-class evidence, inventory mode.
        assert_eq!(res.findings[0].evidence_class, EvidenceClass::Presence);
        assert_eq!(res.mode, Mode::Inventory);
        // No now_epoch supplied → the result is unscored (core stays clockless).
        assert!(res.assurance.is_none());
        assert!(res.exposure.is_none());
        assert!(res.findings[0].computed.is_none());
    }

    #[test]
    fn finds_a_literal_file_without_glob() {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join(".claude.json"), "{}").unwrap();
        let sig = sig_for_current_os("c", "~/.claude.json", MatchKind::File);
        let res = scan_with(home.path(), sig);
        assert_eq!(res.findings.len(), 1);
        assert_eq!(res.findings[0].size_bytes, 2);
    }

    #[test]
    fn missing_path_yields_nothing_and_no_warning() {
        let home = tempfile::tempdir().unwrap();
        let sig = sig_for_current_os("m", "~/.claude.json", MatchKind::File);
        let res = scan_with(home.path(), sig);
        assert!(res.findings.is_empty());
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn directory_size_is_aggregated() {
        let home = tempfile::tempdir().unwrap();
        let d = home.path().join("store");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("a.bin"), vec![0u8; 100]).unwrap();
        fs::write(d.join("sub/b.bin"), vec![0u8; 50]).unwrap();
        let sig = sig_for_current_os("d", "~/store", MatchKind::Dir);
        let res = scan_with(home.path(), sig);
        assert_eq!(res.findings.len(), 1);
        assert_eq!(res.findings[0].size_bytes, 150);
        assert_eq!(res.findings[0].file_count, 2);
    }

    #[test]
    fn match_kind_file_skips_directories() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join("d/nested")).unwrap();
        let sig = sig_for_current_os("k", "~/d/*", MatchKind::File);
        let res = scan_with(home.path(), sig);
        assert!(res.findings.is_empty());
    }

    #[test]
    fn inspection_and_scoring_are_attached() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join("t");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.jsonl"), "x\ny\n").unwrap();
        let mut sig = sig_for_current_os("i", "~/t/**/*.jsonl", MatchKind::File);
        sig.inspector = Some("jsonl_linecount".to_string());
        let res = scan_with(home.path(), sig);
        assert_eq!(res.findings.len(), 1);
        let f = &res.findings[0];
        assert_eq!(f.inspection.as_ref().unwrap().line_count, Some(2));
        // high sensitivity baseline is at least Medium.
        assert!(f.exposure_level >= crate::score::ExposureLevel::Medium);
    }

    #[test]
    fn only_and_exclude_filters() {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join("f.jsonl"), "x").unwrap();
        let sig = sig_for_current_os("keep", "~/f.jsonl", MatchKind::File);

        let excl = ScanConfig {
            extra_definitions: vec![sig.clone()],
            exclude: vec!["keep".to_string()],
            no_slow: true,
            ..ScanConfig::for_home(home.path())
        };
        assert!(scan(&excl).findings.is_empty());

        let only_other = ScanConfig {
            extra_definitions: vec![sig],
            only: vec!["something-else".to_string()],
            no_slow: true,
            ..ScanConfig::for_home(home.path())
        };
        assert!(scan(&only_other).findings.is_empty());
    }
}
