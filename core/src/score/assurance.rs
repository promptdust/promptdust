//! The **Assurance** axis of the dual score (`scoring_model.yaml` §ASSURANCE): *how much to
//! trust* the endpoint Exposure number. It starts at full trust and is docked for what the
//! scan could not see (**coverage gaps** — no one's fault) and for signs of deliberate
//! cleanup (**evasion signals**), and credited for **corroboration**. Its whole reason to
//! exist: absence of an expected store lowers *Assurance*, **never** Exposure toward "clean".
//!
//! This module is the model + detection *logic* (pure functions, golden-tested). Wiring it
//! to a real scan's findings and surfacing it in the output is a separate step (#26); the
//! signals that need later rings (containers/WSL, remote-dev, browser, forensic) are in the
//! catalogs here but only fire once those rings exist.

use serde::{Deserialize, Serialize};

use super::policy;
use crate::model::EvidenceClass;
use crate::platform::DiskEncryption;
use crate::report::Finding;

/// A **coverage gap** — something the scan could not see, through no one's fault. Reported
/// explicitly; each docks Assurance by its `penalty`. Transcribes `coverage_gaps` in
/// `scoring_model.yaml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageGap {
    /// Swap/hibernation encrypted (FileVault/BitLocker/LUKS) — memory content not carvable.
    EncryptedSwapOrFde,
    /// Not enough privilege to read forensic artifacts / memory.
    InsufficientPrivilege,
    /// WSL/containers/VMs present but not scanned — a parallel environment unseen.
    ContainersOrWslNotTraversed,
    /// ssh/Codespaces/remote IDE — AI artifacts live off-box.
    RemoteOrHostedDevDetected,
    /// Browser profiles present but not enumerated/scanned.
    UnscannedBrowserProfiles,
    /// Private/incognito sessions leave no durable trace.
    IncognitoUnobservable,
    /// Content store present but sealed (e.g. the current ChatGPT app).
    EncryptedAtRestStore,
    /// Content store obfuscated (e.g. Windsurf) — presence known, content not extractable.
    ObfuscatedStorePresent,
    /// OS secret store locked — credential contents unverifiable.
    LockedKeychainCredstore,
}

impl CoverageGap {
    /// Assurance points this gap docks.
    #[must_use]
    pub const fn penalty(self) -> u32 {
        match self {
            Self::EncryptedSwapOrFde
            | Self::InsufficientPrivilege
            | Self::ContainersOrWslNotTraversed => 10,
            Self::RemoteOrHostedDevDetected => 12,
            Self::UnscannedBrowserProfiles | Self::IncognitoUnobservable => 8,
            Self::EncryptedAtRestStore | Self::ObfuscatedStorePresent => 6,
            Self::LockedKeychainCredstore => 5,
        }
    }

    /// Stable snake_case identifier (matches the serialized form).
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::EncryptedSwapOrFde => "encrypted_swap_or_fde",
            Self::InsufficientPrivilege => "insufficient_privilege",
            Self::ContainersOrWslNotTraversed => "containers_or_wsl_not_traversed",
            Self::RemoteOrHostedDevDetected => "remote_or_hosted_dev_detected",
            Self::UnscannedBrowserProfiles => "unscanned_browser_profiles",
            Self::IncognitoUnobservable => "incognito_unobservable",
            Self::EncryptedAtRestStore => "encrypted_at_rest_store",
            Self::ObfuscatedStorePresent => "obfuscated_store_present",
            Self::LockedKeychainCredstore => "locked_keychain_credstore",
        }
    }

    /// One-line explanation (why this limited visibility). Verdict-neutral.
    #[must_use]
    pub const fn note(self) -> &'static str {
        match self {
            Self::EncryptedSwapOrFde => {
                "swap/hibernation encrypted (FileVault/BitLocker/LUKS) — memory content not carvable"
            }
            Self::InsufficientPrivilege => {
                "not enough privilege to read forensic artifacts / memory"
            }
            Self::ContainersOrWslNotTraversed => {
                "WSL/containers/VMs present but not scanned — parallel environment unseen"
            }
            Self::RemoteOrHostedDevDetected => {
                "ssh/Codespaces/remote IDE — AI artifacts live off-box"
            }
            Self::UnscannedBrowserProfiles => {
                "browser profiles present but not enumerated/scanned"
            }
            Self::IncognitoUnobservable => "private/incognito sessions leave no durable trace",
            Self::EncryptedAtRestStore => "content store present but sealed",
            Self::ObfuscatedStorePresent => {
                "content store obfuscated — presence known, content not extractable"
            }
            Self::LockedKeychainCredstore => {
                "OS secret store locked — credential contents unverifiable"
            }
        }
    }
}

/// An **evasion signal** — evidence of deliberate cleanup/obfuscation. Reported as a
/// first-class item AND docks Assurance; it **lowers Assurance, never Exposure**
/// (Charter §8.3). Transcribes `evasion_signals` in `scoring_model.yaml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvasionSignal {
    /// HISTFILE unset / PSReadLine history off.
    ShellHistoryDisabled,
    /// History file exists but has zero content — likely wiped.
    HistoryPresentButEmpty,
    /// AI tool installed/used but its expected store is missing/empty and not sealed.
    AppPresentStoreAbsent,
    /// Store dir exists but recently emptied, or many deleted rows in a SQLite freelist.
    TranscriptsRecentlyWiped,
    /// mtime/atime anomalies or uniform-timestamp scrubbing.
    TimestampsScrubbed,
    /// Tool uninstalled but config/creds remnants remain — prior use + cleanup.
    ToolUninstalledRemnantsRemain,
    /// Clipboard-manager history present but emptied.
    ClipboardHistoryCleared,
}

impl EvasionSignal {
    /// Assurance points this signal docks.
    #[must_use]
    pub const fn penalty(self) -> u32 {
        match self {
            Self::ShellHistoryDisabled
            | Self::AppPresentStoreAbsent
            | Self::TranscriptsRecentlyWiped => 12,
            Self::HistoryPresentButEmpty | Self::TimestampsScrubbed => 10,
            Self::ToolUninstalledRemnantsRemain => 6,
            Self::ClipboardHistoryCleared => 4,
        }
    }

    /// Stable snake_case identifier (matches the serialized form).
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::ShellHistoryDisabled => "shell_history_disabled",
            Self::HistoryPresentButEmpty => "history_present_but_empty",
            Self::AppPresentStoreAbsent => "app_present_store_absent",
            Self::TranscriptsRecentlyWiped => "transcripts_recently_wiped",
            Self::TimestampsScrubbed => "timestamps_scrubbed",
            Self::ToolUninstalledRemnantsRemain => "tool_uninstalled_remnants_remain",
            Self::ClipboardHistoryCleared => "clipboard_history_cleared",
        }
    }

    /// One-line explanation. Verdict-neutral (describes the signal, never a safety claim).
    #[must_use]
    pub const fn note(self) -> &'static str {
        match self {
            Self::ShellHistoryDisabled => "HISTFILE unset / PSReadLine history off",
            Self::HistoryPresentButEmpty => "history file exists but zero content — likely wiped",
            Self::AppPresentStoreAbsent => {
                "AI tool installed/used but its expected store is missing/empty and not sealed"
            }
            Self::TranscriptsRecentlyWiped => {
                "store dir exists but recently emptied, or many deleted rows in a SQLite freelist"
            }
            Self::TimestampsScrubbed => "mtime/atime anomalies or uniform-timestamp scrubbing",
            Self::ToolUninstalledRemnantsRemain => {
                "tool uninstalled but config/creds remnants remain — prior use + cleanup"
            }
            Self::ClipboardHistoryCleared => "clipboard-manager history present but emptied",
        }
    }
}

/// The Assurance band. Ordered low < partial < high. Never a verdict — `low` means the look
/// was blind or evaded, **not** that the machine is dirty; `high` means the Exposure number
/// is trustworthy, **not** that nothing is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AssuranceBand {
    /// 0–39 — blind or evaded; a low Exposure here does not mean "found nothing".
    Low,
    /// 40–69 — meaningful gaps in visibility.
    Partial,
    /// 70–100 — broad coverage; the Exposure number can be trusted.
    High,
}

impl AssuranceBand {
    const fn for_score(score: u32) -> Self {
        if score <= policy::ASSURANCE_BAND_LOW_MAX {
            Self::Low
        } else if score <= policy::ASSURANCE_BAND_PARTIAL_MAX {
            Self::Partial
        } else {
            Self::High
        }
    }

    /// A stable lowercase identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Partial => "partial",
            Self::High => "high",
        }
    }
}

/// The endpoint Assurance score: a 0–100 trust magnitude, its band, and the evidence behind
/// it (so every point is re-derivable — Charter A6). Never a verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssuranceScore {
    /// 0–100.
    pub score: u32,
    /// The band `score` falls in.
    pub band: AssuranceBand,
    /// The coverage gaps that docked the score.
    pub coverage_gaps: Vec<CoverageGap>,
    /// The evasion signals that docked the score.
    pub evasion_signals: Vec<EvasionSignal>,
    /// The corroboration bonus applied (after its cap).
    pub corroboration_bonus: u32,
}

/// Endpoint Assurance = `clamp(base − Σcoverage − Σevasion + corroboration, 0, 100)`, each
/// side capped. `corroborated_findings` is the count of findings backed by ≥ 2 independent
/// signal classes (the caller supplies it; signal-class tracking is not built yet, so a real
/// scan passes 0 today — the formula is golden-tested with explicit counts).
#[must_use]
pub fn assurance(
    coverage: &[CoverageGap],
    evasion: &[EvasionSignal],
    corroborated_findings: u32,
) -> AssuranceScore {
    let cpen = coverage
        .iter()
        .map(|g| g.penalty())
        .sum::<u32>()
        .min(policy::COVERAGE_PENALTY_CAP);
    let epen = evasion
        .iter()
        .map(|e| e.penalty())
        .sum::<u32>()
        .min(policy::EVASION_PENALTY_CAP);
    let bonus = corroborated_findings
        .saturating_mul(policy::CORROBORATION_BONUS_PER_FINDING)
        .min(policy::CORROBORATION_BONUS_CAP);
    // clamp(base − cpen − epen + bonus, 0, 100): add the bonus first (no underflow), subtract
    // the capped penalties saturating at 0, then cap at 100.
    let score = (policy::ASSURANCE_BASE + bonus)
        .saturating_sub(cpen + epen)
        .min(100);
    AssuranceScore {
        score,
        band: AssuranceBand::for_score(score),
        coverage_gaps: coverage.to_vec(),
        evasion_signals: evasion.to_vec(),
        corroboration_bonus: bonus,
    }
}

/// The observed state of a tool's content store (metadata only — no content is read to
/// determine it). One value per store rules out the contradictory combinations that separate
/// booleans would admit (e.g. sealed *and* empty).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentStore {
    /// A non-empty, readable content store was found.
    Readable,
    /// Present but sealed — content not extractable (the store reports its content as not
    /// readable). `obfuscated` picks the coverage-gap id.
    Sealed {
        /// Obfuscated (e.g. Windsurf) rather than encrypted-at-rest (e.g. ChatGPT).
        obfuscated: bool,
    },
    /// Found but empty (size 0 / 0 rows) — a wipe signal.
    Emptied,
    /// The tool is present but no content store was found at all.
    Absent,
}

/// One tool's metadata observations for Assurance detection — a view of *metadata only* (no
/// content is read to build it).
#[derive(Debug, Clone, Copy)]
pub struct AssuranceInput {
    /// The definition's declared deepest evidence class — does this tool normally store content?
    pub max_evidence_class: EvidenceClass,
    /// A presence/usage signal fired for this tool (it is installed / has been used).
    pub present: bool,
    /// The observed state of this tool's content store.
    pub content_store: ContentStore,
}

/// Detect the Assurance signals observable **today** from scan metadata: encrypted swap/FDE
/// (from disk encryption), and the **absence-rule** fork per tool — a content-class tool that
/// is present but whose content could not be read is either a *coverage gap* (sealed) or an
/// *evasion signal* (emptied/absent), **never** a reason to lower Exposure. Signals that need
/// later rings stay in the catalog but do not fire here.
#[must_use]
pub fn detect_assurance_signals(
    disk_encryption: DiskEncryption,
    inputs: &[AssuranceInput],
) -> (Vec<CoverageGap>, Vec<EvasionSignal>) {
    let mut coverage = Vec::new();
    let mut evasion = Vec::new();

    // Machine-level: encrypted swap / full-disk encryption limits forensic memory carving.
    if disk_encryption == DiskEncryption::On {
        coverage.push(CoverageGap::EncryptedSwapOrFde);
    }

    for inp in inputs {
        // Absence-rule (Charter §8.3; `absence_rules` in scoring_model.yaml): a content-class
        // tool is present, but no readable content store was found. This is NOT "clean" — it
        // is sealed (coverage) or the store was emptied/absent (evasion). Neither lowers
        // Exposure.
        let content_expected = matches!(inp.max_evidence_class, EvidenceClass::Content);
        if content_expected && inp.present {
            match inp.content_store {
                ContentStore::Readable => {}
                ContentStore::Sealed { obfuscated } => coverage.push(if obfuscated {
                    CoverageGap::ObfuscatedStorePresent
                } else {
                    CoverageGap::EncryptedAtRestStore
                }),
                // A not-sealed store with no readable content — whether found-but-empty or
                // absent — is `app_present_store_absent` (its note covers "missing/empty"),
                // per the doctrine's absence_rules else-branch. The finer wipe signals
                // (`transcripts_recently_wiped` / `history_present_but_empty`) require
                // temporal/forensic evidence (freelist rows, mtime anomalies) not observable
                // at this ring, so they stay inert rather than overclaim "recently wiped".
                ContentStore::Emptied | ContentStore::Absent => {
                    evasion.push(EvasionSignal::AppPresentStoreAbsent);
                }
            }
        }
    }

    (coverage, evasion)
}

/// Build the per-store Assurance inputs from a scan's findings. `content_max(id)` returns a
/// definition's declared deepest evidence class by `definition_id` — a store is content-capable
/// when that is `Content`. At Ring 0 the observable absence-rule input is an **empty**
/// content-capable store (`size_bytes == 0`, present but its content is gone); sealed /
/// obfuscated detection awaits a per-epoch `content_readable` flag and stays inert (its
/// `ContentStore::Sealed` branch is never produced here). A tool that is installed but whose
/// store never matched *at all* is likewise unobservable at Ring 0 — there is no `Finding` to
/// iterate — so `ContentStore::Absent` (the "store missing entirely" half of the absence
/// rule) is not produced yet either. No content is read.
#[must_use]
pub fn assurance_inputs(
    findings: &[Finding],
    content_max: impl Fn(&str) -> Option<EvidenceClass>,
) -> Vec<AssuranceInput> {
    findings
        .iter()
        .filter_map(|f| {
            let max = content_max(&f.definition_id)?;
            if max != EvidenceClass::Content {
                return None;
            }
            let content_store = if f.size_bytes == 0 {
                ContentStore::Emptied
            } else {
                ContentStore::Readable
            };
            Some(AssuranceInput {
                max_evidence_class: max,
                present: true,
                content_store,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Keep these two arrays in sync with the enums: a new variant not added here silently
    // escapes the catalog/cap tests below (there is no variant-count reflection without a dep).
    const ALL_COVERAGE: [CoverageGap; 9] = [
        CoverageGap::EncryptedSwapOrFde,
        CoverageGap::InsufficientPrivilege,
        CoverageGap::ContainersOrWslNotTraversed,
        CoverageGap::RemoteOrHostedDevDetected,
        CoverageGap::UnscannedBrowserProfiles,
        CoverageGap::IncognitoUnobservable,
        CoverageGap::EncryptedAtRestStore,
        CoverageGap::ObfuscatedStorePresent,
        CoverageGap::LockedKeychainCredstore,
    ];
    const ALL_EVASION: [EvasionSignal; 7] = [
        EvasionSignal::ShellHistoryDisabled,
        EvasionSignal::HistoryPresentButEmpty,
        EvasionSignal::AppPresentStoreAbsent,
        EvasionSignal::TranscriptsRecentlyWiped,
        EvasionSignal::TimestampsScrubbed,
        EvasionSignal::ToolUninstalledRemnantsRemain,
        EvasionSignal::ClipboardHistoryCleared,
    ];

    #[test]
    fn catalog_ids_round_trip_and_are_documented() {
        // The serialized form equals id(); every entry has a non-empty note and a real penalty.
        for g in ALL_COVERAGE {
            assert_eq!(
                serde_json::to_string(&g).unwrap(),
                format!("\"{}\"", g.id())
            );
            assert!(
                !g.note().trim().is_empty(),
                "coverage {} has empty note",
                g.id()
            );
            assert!(g.penalty() > 0, "coverage {} has zero penalty", g.id());
        }
        for e in ALL_EVASION {
            assert_eq!(
                serde_json::to_string(&e).unwrap(),
                format!("\"{}\"", e.id())
            );
            assert!(
                !e.note().trim().is_empty(),
                "evasion {} has empty note",
                e.id()
            );
            assert!(e.penalty() > 0, "evasion {} has zero penalty", e.id());
        }
    }

    #[test]
    fn penalties_match_the_doctrine() {
        // Pin every penalty to scoring_model.yaml. The caps swallow individual values in the
        // aggregate tests, so without this a typo in any penalty not used by golden scenario C
        // would pass every other test.
        use CoverageGap as C;
        use EvasionSignal as E;
        let coverage = [
            (C::EncryptedSwapOrFde, 10),
            (C::InsufficientPrivilege, 10),
            (C::ContainersOrWslNotTraversed, 10),
            (C::RemoteOrHostedDevDetected, 12),
            (C::UnscannedBrowserProfiles, 8),
            (C::IncognitoUnobservable, 8),
            (C::EncryptedAtRestStore, 6),
            (C::ObfuscatedStorePresent, 6),
            (C::LockedKeychainCredstore, 5),
        ];
        for (g, want) in coverage {
            assert_eq!(g.penalty(), want, "coverage {} penalty", g.id());
        }
        let evasion = [
            (E::ShellHistoryDisabled, 12),
            (E::HistoryPresentButEmpty, 10),
            (E::AppPresentStoreAbsent, 12),
            (E::TranscriptsRecentlyWiped, 12),
            (E::TimestampsScrubbed, 10),
            (E::ToolUninstalledRemnantsRemain, 6),
            (E::ClipboardHistoryCleared, 4),
        ];
        for (e, want) in evasion {
            assert_eq!(e.penalty(), want, "evasion {} penalty", e.id());
        }
    }

    #[test]
    fn full_trust_when_nothing_is_docked() {
        let a = assurance(&[], &[], 0);
        assert_eq!(a.score, 100);
        assert_eq!(a.band, AssuranceBand::High);
        assert_eq!(a.corroboration_bonus, 0);
    }

    #[test]
    fn assurance_echoes_evidence_and_applies_the_bonus_rate() {
        let cov = [CoverageGap::EncryptedAtRestStore]; // 6
        let eva = [EvasionSignal::ShellHistoryDisabled]; // 12
        let a = assurance(&cov, &eva, 2);
        // The evidence lists are echoed verbatim — this is what #26 renders into the report.
        assert_eq!(a.coverage_gaps, cov);
        assert_eq!(a.evasion_signals, eva);
        // +4 per corroborated finding, below the cap: 2 → 8 (pins the rate, not just the cap).
        assert_eq!(a.corroboration_bonus, 8);
        // score = 100 − 6 − 12 + 8 = 90.
        assert_eq!(a.score, 90);
    }

    #[test]
    fn coverage_penalty_caps_at_50() {
        let raw: u32 = ALL_COVERAGE.iter().map(|g| g.penalty()).sum();
        assert!(
            raw > 50,
            "precondition: raw coverage {raw} must exceed the cap"
        );
        assert_eq!(
            assurance(&ALL_COVERAGE, &[], 0).score,
            50,
            "coverage penalty must cap at 50 → 100-50"
        );
    }

    #[test]
    fn evasion_penalty_caps_at_45() {
        let raw: u32 = ALL_EVASION.iter().map(|e| e.penalty()).sum();
        assert!(
            raw > 45,
            "precondition: raw evasion {raw} must exceed the cap"
        );
        assert_eq!(
            assurance(&[], &ALL_EVASION, 0).score,
            55,
            "evasion penalty must cap at 45 → 100-45"
        );
    }

    #[test]
    fn corroboration_bonus_caps_at_20() {
        // All coverage docks 50 (base → 50); 10 findings would add 40 but the bonus caps at
        // 20 → 70, not 90. Pins the corroboration cap distinctly from the score clamp.
        let a = assurance(&ALL_COVERAGE, &[], 10);
        assert_eq!(a.corroboration_bonus, 20);
        assert_eq!(a.score, 70);
    }

    #[test]
    fn score_never_underflows_or_exceeds_100() {
        // The caps bound the total dock at 50+45=95, so the practical floor is 5 (base 100);
        // `saturating_sub` is a defensive guard should a future cap rise past 100. Huge
        // corroboration still clamps at 100.
        assert_eq!(assurance(&ALL_COVERAGE, &ALL_EVASION, 0).score, 5); // 100 - 50 - 45
        assert_eq!(assurance(&[], &[], 1000).score, 100); // bonus capped 20, still ≤ 100
    }

    #[test]
    fn bands_map_at_their_edges() {
        for (score, band) in [
            (0, AssuranceBand::Low),
            (39, AssuranceBand::Low),
            (40, AssuranceBand::Partial),
            (69, AssuranceBand::Partial),
            (70, AssuranceBand::High),
            (100, AssuranceBand::High),
        ] {
            assert_eq!(AssuranceBand::for_score(score), band, "band for {score}");
        }
    }

    fn present(max_ec: EvidenceClass, store: ContentStore) -> AssuranceInput {
        AssuranceInput {
            max_evidence_class: max_ec,
            present: true,
            content_store: store,
        }
    }

    #[test]
    fn absence_rule_sealed_store_is_coverage_not_evasion() {
        let (cov, eva) = detect_assurance_signals(
            DiskEncryption::Unknown,
            &[present(
                EvidenceClass::Content,
                ContentStore::Sealed { obfuscated: false },
            )],
        );
        assert_eq!(cov, vec![CoverageGap::EncryptedAtRestStore]);
        assert!(eva.is_empty(), "sealed = visibility limit, never evasion");
    }

    #[test]
    fn absence_rule_obfuscated_store_picks_the_obfuscated_gap() {
        let (cov, _) = detect_assurance_signals(
            DiskEncryption::Unknown,
            &[present(
                EvidenceClass::Content,
                ContentStore::Sealed { obfuscated: true },
            )],
        );
        assert_eq!(cov, vec![CoverageGap::ObfuscatedStorePresent]);
    }

    #[test]
    fn absence_rule_emptied_and_absent_both_report_store_absent() {
        // Per the doctrine's absence_rules, a not-sealed store with no readable content is
        // `app_present_store_absent` whether it was found-but-empty or missing entirely.
        for state in [ContentStore::Emptied, ContentStore::Absent] {
            let (cov, eva) = detect_assurance_signals(
                DiskEncryption::Unknown,
                &[present(EvidenceClass::Content, state)],
            );
            assert!(cov.is_empty(), "{state:?} is evasion, not a coverage gap");
            assert_eq!(eva, vec![EvasionSignal::AppPresentStoreAbsent], "{state:?}");
        }
    }

    #[test]
    fn readable_store_fires_no_absence_signal() {
        let (cov, eva) = detect_assurance_signals(
            DiskEncryption::Unknown,
            &[present(EvidenceClass::Content, ContentStore::Readable)],
        );
        assert!(
            cov.is_empty() && eva.is_empty(),
            "a readable store is no gap, no evasion"
        );
    }

    #[test]
    fn non_content_tool_never_triggers_the_absence_rule() {
        // Only content-class tools are expected to have a content store; presence/usage tools
        // with no store are not an absence signal.
        let (cov, eva) = detect_assurance_signals(
            DiskEncryption::Unknown,
            &[
                present(EvidenceClass::Presence, ContentStore::Absent),
                present(EvidenceClass::Usage, ContentStore::Absent),
            ],
        );
        assert!(cov.is_empty() && eva.is_empty());
    }

    #[test]
    fn encrypted_disk_is_a_coverage_gap_only_when_on() {
        let (cov, eva) = detect_assurance_signals(DiskEncryption::On, &[]);
        assert_eq!(cov, vec![CoverageGap::EncryptedSwapOrFde]);
        assert!(eva.is_empty());
        assert!(detect_assurance_signals(DiskEncryption::Off, &[])
            .0
            .is_empty());
        assert!(detect_assurance_signals(DiskEncryption::Unknown, &[])
            .0
            .is_empty());
    }

    #[test]
    fn absent_store_on_a_tool_not_present_fires_nothing() {
        // The `present` guard: a content-class definition whose tool was never detected is not
        // an absence signal — you cannot call a store "missing" for a tool that is not there.
        let (cov, eva) = detect_assurance_signals(
            DiskEncryption::Unknown,
            &[AssuranceInput {
                max_evidence_class: EvidenceClass::Content,
                present: false,
                content_store: ContentStore::Absent,
            }],
        );
        assert!(cov.is_empty() && eva.is_empty());
    }
}
