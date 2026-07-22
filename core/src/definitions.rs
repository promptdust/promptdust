//! Loading the definition database: the bundled (compiled-in) set plus any
//! user-supplied files. Malformed files are skipped with a recorded warning; they
//! never abort a load (FR-9).

use std::path::Path;

use include_dir::{include_dir, Dir};
use serde::Deserialize;

use crate::model::Definition;
use crate::report::ScanWarning;

/// The definition database embedded at compile time so the tool works fully offline.
static BUNDLED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/definitions");

/// A definition file may hold a single object or an array of them.
#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    Many(Vec<Definition>),
    One(Box<Definition>),
}

impl OneOrMany {
    fn into_vec(self) -> Vec<Definition> {
        match self {
            Self::Many(v) => v,
            Self::One(s) => vec![*s],
        }
    }
}

/// The outcome of loading definitions.
#[derive(Debug, Default)]
pub struct Loaded {
    /// Successfully parsed definitions.
    pub definitions: Vec<Definition>,
    /// The bundled definition-DB version (CalVer), or `"unknown"`.
    pub db_version: String,
    /// Non-fatal load problems (malformed files).
    pub warnings: Vec<ScanWarning>,
}

fn is_definition_file(name: &str) -> bool {
    name.ends_with(".json") && !name.starts_with('_')
}

fn parse_into(
    source: &str,
    contents: &str,
    out: &mut Vec<Definition>,
    warnings: &mut Vec<ScanWarning>,
) {
    match serde_json::from_str::<OneOrMany>(contents) {
        Ok(parsed) => out.extend(parsed.into_vec()),
        Err(e) => warnings.push(ScanWarning::new_named(
            source,
            format!("invalid definition JSON: {e}"),
        )),
    }
}

/// Parse definition JSON (a single object or an array) into a vector, or a serde
/// error. Used by `promptdust definitions validate` and by loaders.
///
/// # Errors
/// Returns the underlying `serde_json` error if the text is not a valid definition
/// object or array of definition objects.
pub fn parse_str(contents: &str) -> Result<Vec<Definition>, serde_json::Error> {
    serde_json::from_str::<OneOrMany>(contents).map(OneOrMany::into_vec)
}

/// Load the compiled-in definition database.
#[must_use]
pub fn load_bundled() -> Loaded {
    let mut definitions = Vec::new();
    let mut warnings = Vec::new();

    for file in BUNDLED.files() {
        let name = file
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !is_definition_file(name) {
            continue;
        }
        if let Some(contents) = file.contents_utf8() {
            parse_into(name, contents, &mut definitions, &mut warnings);
        }
    }

    let db_version = BUNDLED
        .get_file("VERSION")
        .and_then(include_dir::File::contents_utf8)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Loaded {
        definitions,
        db_version,
        warnings,
    }
}

/// Load user definition files from a directory (if it exists), appending to `loaded`.
pub fn load_user_dir(dir: &Path, loaded: &mut Loaded) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return, // absent user dir is normal, not an error
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        if !is_definition_file(&name) {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(contents) => parse_into(
                &name,
                &contents,
                &mut loaded.definitions,
                &mut loaded.warnings,
            ),
            Err(e) => loaded
                .warnings
                .push(ScanWarning::new_named(&name, format!("unreadable: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_loads_claude_code() {
        let loaded = load_bundled();
        assert!(
            loaded
                .definitions
                .iter()
                .any(|s| s.id == "claude-code-transcripts"),
            "expected the bundled claude-code definition"
        );
        assert!(
            loaded.warnings.is_empty(),
            "bundled definitions must be clean"
        );
        assert_ne!(loaded.db_version, "unknown");
        // The template file (leading underscore) must not be loaded.
        assert!(!loaded
            .definitions
            .iter()
            .any(|s| s.id.starts_with("example-tool")));
    }

    #[test]
    fn malformed_user_file_warns_not_panics() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("broken.json"), "{ not json").unwrap();
        std::fs::write(dir.path().join("_skip.json"), "ignored").unwrap();
        let mut loaded = Loaded::default();
        load_user_dir(dir.path(), &mut loaded);
        assert_eq!(loaded.definitions.len(), 0);
        assert_eq!(loaded.warnings.len(), 1, "only broken.json should warn");
    }

    #[test]
    fn absent_user_dir_is_silent() {
        let mut loaded = Loaded::default();
        load_user_dir(Path::new("/nonexistent/promptdust/xyz"), &mut loaded);
        assert!(loaded.warnings.is_empty());
    }

    #[test]
    fn bundled_catalog_is_well_formed() {
        let loaded = load_bundled();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert!(
            loaded.definitions.len() >= 10,
            "catalog should be non-trivial"
        );

        let mut ids = std::collections::HashSet::new();
        for s in &loaded.definitions {
            assert_eq!(s.schema_version, 1, "{}: schema_version", s.id);
            assert!(!s.id.is_empty(), "empty id");
            assert!(!s.tool.is_empty(), "{}: empty tool", s.id);
            assert!(!s.why.is_empty(), "{}: empty why", s.id);
            assert!(!s.platforms.is_empty(), "{}: no platforms", s.id);
            assert!(!s.paths.is_empty(), "{}: no paths", s.id);
            assert!(ids.insert(s.id.clone()), "duplicate id: {}", s.id);
        }
    }
}
