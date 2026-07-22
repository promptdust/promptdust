//! The definition data model — the declarative description of one AI-data artifact.
//!
//! These types deserialize the JSON definition database (see
//! `core/definitions/schema/definition.schema.json`). Unknown fields are ignored on
//! purpose (forward-compatibility); typo-catching is done by the CI validator, not
//! by rejecting unknown fields here.

use serde::{Deserialize, Serialize};

/// An operating system a definition or path applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    /// Apple macOS.
    Macos,
    /// Linux.
    Linux,
    /// Microsoft Windows.
    Windows,
}

impl Platform {
    /// The platform this binary was compiled for, or `None` on an unsupported OS.
    #[must_use]
    pub const fn current() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::Macos)
        } else if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else if cfg!(target_os = "windows") {
            Some(Self::Windows)
        } else {
            None
        }
    }
}

/// What an artifact contains (Axis 1 of the model).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Verbatim conversation history.
    Transcript,
    /// Derived / cached app or model data.
    Cache,
    /// Vector index / embeddings of user content.
    EmbeddingIndex,
    /// Configuration file that may hold API keys / tokens.
    ConfigWithSecrets,
    /// Local debug / telemetry trace.
    Log,
    /// File or media ingested into a conversation.
    Attachment,
}

/// The on-disk format of an artifact (drives which inspector may run).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// Newline-delimited JSON.
    Jsonl,
    /// SQLite database.
    Sqlite,
    /// LevelDB / IndexedDB store (metadata-only inspection).
    Leveldb,
    /// Plain text.
    Plaintext,
    /// Apple property list.
    Plist,
    /// JSON document.
    Json,
    /// Opaque binary.
    Binary,
    /// A directory of files.
    Dir,
}

/// Baseline sensitivity of an artifact, before exposure amplifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Sensitivity {
    /// Low baseline sensitivity.
    Low,
    /// Medium baseline sensitivity.
    Medium,
    /// High baseline sensitivity.
    High,
}

/// How confident we are that a definition is accurate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Confirmed against a real install.
    Verified,
    /// Documented but unconfirmed.
    Likely,
    /// Community-reported; unconfirmed.
    Unverified,
}

/// Whether a path pattern is expected to match a file, a directory, or either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchKind {
    /// Match regular files only.
    File,
    /// Match directories only.
    Dir,
    /// Match either.
    Either,
}

/// One read-only path selector within a definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PathPattern {
    /// Narrow this pattern to one OS (optional; otherwise applies to all of the
    /// definition's `platforms`).
    #[serde(default)]
    pub os: Option<Platform>,
    /// The pattern. `~`, `$ENV`/`${ENV}`, and globs (`*`, `**`, `?`, `[..]`) allowed.
    pub pattern: String,
    /// Whether it targets a file, directory, or either.
    #[serde(rename = "match")]
    pub match_kind: MatchKind,
}

/// The deepest kind of evidence a definition's artifact can yield (the Ring model):
/// mere *presence*, *usage*/timeline signals, or *content*. Content is only ever
/// classified in-memory inside a consented ring and never emitted raw (ADR-017).
///
/// Ordered by depth: `Presence < Usage < Content`, so a finding's reached depth can
/// be capped by a definition's declared `max_evidence_class` with `.min()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceClass {
    /// Existence, size, timestamps, counts, structural shape only.
    Presence,
    /// Whether/when it ran; timeline; persistence markers.
    Usage,
    /// The stored values themselves (type-classified in-memory, never emitted raw).
    Content,
}

impl EvidenceClass {
    /// A stable lowercase identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Presence => "presence",
            Self::Usage => "usage",
            Self::Content => "content",
        }
    }
}

/// A kind of sensitive data an artifact may hold — used to classify content *type*,
/// never to emit the values themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SensitivityType {
    /// Credentials / API keys / tokens.
    Secret,
    /// Personally identifiable information.
    Pii,
    /// Protected health information.
    Phi,
    /// Source code.
    Source,
}

/// How the artifact changes over time — informs recency/decay in the exposure score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Volatility {
    /// Long-lived; rarely rewritten.
    Stable,
    /// Rewritten/rotated in place (e.g. a rolling log or leveldb).
    Rolling,
    /// Short-lived / frequently recreated.
    Ephemeral,
}

/// A read-only way to observe the producing tool's version — metadata-only, never
/// executes anything.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct VersionDetect {
    /// Where a version string can be read from (a path, plist key, or JSON pointer).
    pub source: String,
    /// Optional hint for how to read `source` (e.g. `plist-key`, `json-pointer`).
    #[serde(default)]
    pub kind: Option<String>,
}

/// One historical storage location/era for a tool's data. Tools move their stores
/// across versions; older epochs may still hold recoverable data.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct StorageEpoch {
    /// A label for this era (e.g. a version range or date boundary).
    pub label: String,
    /// Optional note about what this epoch covers or when it applied.
    #[serde(default)]
    pub note: Option<String>,
}

/// One known AI-data artifact location.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Definition {
    /// Definition-schema version (always 1 today).
    pub schema_version: u32,
    /// Stable, unique, kebab-case id.
    pub id: String,
    /// Human display name of the tool.
    pub tool: String,
    /// Optional vendor.
    #[serde(default)]
    pub vendor: Option<String>,
    /// Operating systems this definition applies to.
    pub platforms: Vec<Platform>,
    /// One or more path selectors.
    pub paths: Vec<PathPattern>,
    /// What the artifact contains.
    pub category: Category,
    /// On-disk format.
    pub format: Format,
    /// Baseline sensitivity.
    pub sensitivity: Sensitivity,
    /// Optional metadata-only inspector name.
    #[serde(default)]
    pub inspector: Option<String>,
    /// Optional inspector parameters.
    #[serde(default)]
    pub inspector_args: Option<serde_json::Value>,
    /// Optional note about the tool's own retention control.
    #[serde(default)]
    pub retention_hint: Option<String>,
    /// One sentence: what it contains and why it matters.
    pub why: String,
    /// Optional non-destructive guidance strings.
    #[serde(default)]
    pub guidance: Vec<String>,
    /// Optional confidence tier.
    #[serde(default)]
    pub confidence: Option<Confidence>,
    /// Optional provenance links.
    #[serde(default)]
    pub references: Vec<String>,

    // ── Enriched fact model (schema v1, additive; all optional — ADR-019). These
    //    carry richer per-store facts for the future dual score; policy stays in
    //    `score.rs`. v1 files without them still parse (serde defaults). ──
    /// Base exposure weight for the endpoint score (doctrine scale 0–10; see
    /// `score::policy`). Bundled definitions currently declare 0–100 pending a re-scale.
    /// Absent = no contribution.
    #[serde(default)]
    pub base_weight: Option<u8>,
    /// The deepest evidence class this artifact can yield.
    #[serde(default)]
    pub max_evidence_class: Option<EvidenceClass>,
    /// Kinds of sensitive data the artifact may hold (for content-type classification).
    #[serde(default)]
    pub sensitivity_types: Vec<SensitivityType>,
    /// How the artifact changes over time.
    #[serde(default)]
    pub volatility: Option<Volatility>,
    /// A read-only way to observe the producing tool's version.
    #[serde(default)]
    pub version_detect: Option<VersionDetect>,
    /// Known historical storage locations/eras for this tool's data.
    #[serde(default)]
    pub storage_epochs: Vec<StorageEpoch>,
}

#[cfg(test)]
impl Default for Definition {
    /// Test-only: an empty definition (schema v1) used to fill optional/new fields at
    /// construction sites via `..Default::default()`. Gated behind `cfg(test)` so it
    /// isn't public API — the required fields carry placeholder values that callers
    /// set explicitly. Non-test code builds definitions by deserializing the JSON DB.
    fn default() -> Self {
        Self {
            schema_version: 1,
            id: String::new(),
            tool: String::new(),
            vendor: None,
            platforms: Vec::new(),
            paths: Vec::new(),
            category: Category::Cache,
            format: Format::Json,
            sensitivity: Sensitivity::Low,
            inspector: None,
            inspector_args: None,
            retention_hint: None,
            why: String::new(),
            guidance: Vec::new(),
            confidence: None,
            references: Vec::new(),
            base_weight: None,
            max_evidence_class: None,
            sensitivity_types: Vec::new(),
            volatility: None,
            version_detect: None,
            storage_epochs: Vec::new(),
        }
    }
}

impl Definition {
    /// Does this definition apply to the given platform?
    #[must_use]
    pub fn applies_to(&self, platform: Platform) -> bool {
        self.platforms.contains(&platform)
    }

    /// The path patterns that apply to the given platform (respecting per-path `os`).
    pub fn applicable_paths(&self, platform: Platform) -> impl Iterator<Item = &PathPattern> {
        self.paths
            .iter()
            .filter(move |p| p.os.map_or(true, |os| os == platform))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_minimal_definition() {
        let json = r#"{
            "schema_version": 1, "id": "x-y", "tool": "X", "platforms": ["macos"],
            "paths": [{"pattern": "~/x", "match": "file"}],
            "category": "transcript", "format": "jsonl", "sensitivity": "high",
            "why": "because"
        }"#;
        let sig: Definition = serde_json::from_str(json).unwrap();
        assert_eq!(sig.id, "x-y");
        assert_eq!(sig.category, Category::Transcript);
        assert!(sig.guidance.is_empty());
        assert!(sig.confidence.is_none());
    }

    #[test]
    fn ignores_unknown_fields_for_forward_compat() {
        let json = r#"{
            "schema_version": 1, "id": "x", "tool": "X", "platforms": ["linux"],
            "paths": [{"pattern": "~/x", "match": "either"}],
            "category": "log", "format": "plaintext", "sensitivity": "low",
            "why": "b", "some_future_field": 42
        }"#;
        let sig: Definition = serde_json::from_str(json).unwrap();
        assert_eq!(sig.id, "x");
    }

    #[test]
    fn snake_case_category_names() {
        assert_eq!(
            serde_json::to_string(&Category::ConfigWithSecrets).unwrap(),
            "\"config_with_secrets\""
        );
        assert_eq!(
            serde_json::to_string(&Category::EmbeddingIndex).unwrap(),
            "\"embedding_index\""
        );
    }

    #[test]
    fn applicable_paths_respects_per_path_os() {
        let json = r#"{
            "schema_version": 1, "id": "x", "tool": "X", "platforms": ["macos","linux"],
            "paths": [
                {"os":"macos","pattern":"~/mac","match":"file"},
                {"os":"linux","pattern":"~/lin","match":"file"},
                {"pattern":"~/both","match":"file"}
            ],
            "category": "cache", "format": "json", "sensitivity": "medium", "why": "b"
        }"#;
        let sig: Definition = serde_json::from_str(json).unwrap();
        let mac: Vec<_> = sig
            .applicable_paths(Platform::Macos)
            .map(|p| p.pattern.as_str())
            .collect();
        assert_eq!(mac, vec!["~/mac", "~/both"]);
    }

    #[test]
    fn deserializes_enriched_fields() {
        let json = r#"{
            "schema_version": 1, "id": "x", "tool": "X", "platforms": ["macos"],
            "paths": [{"pattern": "~/x", "match": "file"}],
            "category": "config_with_secrets", "format": "json", "sensitivity": "high",
            "why": "b",
            "base_weight": 90,
            "max_evidence_class": "content",
            "sensitivity_types": ["secret", "source"],
            "volatility": "rolling",
            "version_detect": {"source": "~/x/version", "kind": "plist-key"},
            "storage_epochs": [{"label": "v1", "note": "legacy path"}, {"label": "v2"}]
        }"#;
        let sig: Definition = serde_json::from_str(json).unwrap();
        assert_eq!(sig.base_weight, Some(90));
        assert_eq!(sig.max_evidence_class, Some(EvidenceClass::Content));
        assert_eq!(
            sig.sensitivity_types,
            vec![SensitivityType::Secret, SensitivityType::Source]
        );
        assert_eq!(sig.volatility, Some(Volatility::Rolling));
        let vd = sig.version_detect.expect("version_detect present");
        assert_eq!(vd.source, "~/x/version");
        assert_eq!(vd.kind.as_deref(), Some("plist-key"));
        assert_eq!(sig.storage_epochs.len(), 2);
        assert_eq!(sig.storage_epochs[0].label, "v1");
        assert_eq!(sig.storage_epochs[0].note.as_deref(), Some("legacy path"));
        assert_eq!(sig.storage_epochs[1].note, None);
    }

    #[test]
    fn enriched_fields_default_when_absent() {
        // A plain v1 definition (no enriched fields) still parses; new fields default empty.
        let json = r#"{
            "schema_version": 1, "id": "x", "tool": "X", "platforms": ["macos"],
            "paths": [{"pattern": "~/x", "match": "file"}],
            "category": "cache", "format": "json", "sensitivity": "low", "why": "b"
        }"#;
        let sig: Definition = serde_json::from_str(json).unwrap();
        assert_eq!(sig.base_weight, None);
        assert!(sig.max_evidence_class.is_none());
        assert!(sig.sensitivity_types.is_empty());
        assert!(sig.volatility.is_none());
        assert!(sig.version_detect.is_none());
        assert!(sig.storage_epochs.is_empty());
    }

    #[test]
    fn evidence_class_is_ordered_by_depth() {
        assert!(EvidenceClass::Presence < EvidenceClass::Usage);
        assert!(EvidenceClass::Usage < EvidenceClass::Content);
        // The cap used at scan time: a reached depth is limited by the declared max.
        assert_eq!(
            EvidenceClass::Content.min(EvidenceClass::Presence),
            EvidenceClass::Presence
        );
    }
}
