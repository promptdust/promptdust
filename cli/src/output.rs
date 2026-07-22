//! CLI output: builds the JSON contract via the shared `promptdust_core::output`
//! document, supplying the timestamp/host the clockless core omits.

use promptdust_core::definitions::Loaded;
use promptdust_core::model::Definition;
use promptdust_core::{DiagnosticsDocument, Host, OutputDocument, ScanResult};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Render a scan result as the pretty-printed JSON contract.
#[must_use]
pub fn to_json(result: &ScanResult) -> String {
    OutputDocument::new(result, now_rfc3339(), host()).to_json_pretty()
}

/// Render a scan as the redacted, path-free diagnostics bundle for a bug report — the
/// count-only projection plus this host's OS/arch, the tool version, and the scan duration.
#[must_use]
pub fn diagnostics_json(result: &ScanResult, scan_duration_ms: u64) -> String {
    DiagnosticsDocument::new(
        result,
        now_rfc3339(),
        env!("CARGO_PKG_VERSION").to_string(),
        host(),
        Some(scan_duration_ms),
    )
    .to_json_pretty()
}

fn host() -> Host {
    Host {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        os_version: os_version(),
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
}

fn os_version() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()?;
        if out.status.success() {
            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
            return (!v.is_empty()).then_some(v);
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Render the loaded definitions DB as the public `catalog.json` — the website's data
/// contract. A **public projection**: only the fields a person needs to understand where a
/// tool stores data. Internal scoring/detection inputs (`base_weight`, `max_evidence_class`,
/// `volatility`, `version_detect`, `storage_epochs`, `inspector*`) are deliberately omitted,
/// and no conversation content is ever included (metadata-only).
///
/// Two views over the same data: a flat `definitions[]` (one record per store) and a `tools[]`
/// rollup (one entry per tool, for the site's per-tool pages). Both are emitted in a
/// **deterministic order** — definitions by `id`, tools by case-insensitive name — so a
/// committed or consumed `catalog.json` stays stable and diffable.
#[must_use]
pub fn catalog_json(loaded: &Loaded) -> String {
    // Stable, diffable contract: a deterministic id order, independent of DB load order.
    let mut sigs: Vec<&Definition> = loaded.definitions.iter().collect();
    sigs.sort_by(|a, b| a.id.cmp(&b.id));

    let definitions: Vec<serde_json::Value> = sigs
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "tool": s.tool,
                "vendor": s.vendor,
                "platforms": s.platforms,
                "paths": s.paths,
                "category": s.category,
                "format": s.format,
                "sensitivity": s.sensitivity,
                "why": s.why,
                "guidance": s.guidance,
                "confidence": s.confidence,
                "references": s.references,
                "sensitivity_types": s.sensitivity_types,
            })
        })
        .collect();

    let tools = tool_rollups(&sigs);

    serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": promptdust_core::SCHEMA_VERSION,
        "db_version": loaded.db_version,
        "definition_count": definitions.len(),
        "tool_count": tools.len(),
        "definitions": definitions,
        "tools": tools,
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

/// One rollup entry per tool (grouped by `tool`, in case-insensitive name order) for the
/// catalog's per-tool pages: the tool's `vendor`, the sorted union of its stores'
/// `platforms`/`categories`, a non-lossy `confidence_counts` distribution over its stores,
/// and the `definition_ids` that join back to `definitions[]`. A single tool-level `confidence`
/// *tier* is deliberately not emitted — it would bury a verified store behind a weaker sibling
/// and contradict `definitions[]`; the count distribution is the honest tool-level signal. Other
/// per-store facts (`sensitivity`, `paths`, …) stay in `definitions[]` (join by id). Derived
/// purely from `definitions` — a rollup, not new data.
fn tool_rollups(sigs: &[&Definition]) -> Vec<serde_json::Value> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut by_tool: BTreeMap<&str, Vec<&Definition>> = BTreeMap::new();
    for &s in sigs {
        by_tool.entry(s.tool.as_str()).or_default().push(s);
    }

    let mut tools: Vec<(&str, serde_json::Value)> = by_tool
        .into_iter()
        .map(|(tool, mut stores)| {
            // Sort the tool's stores by id once: makes both `definition_ids` and the `vendor`
            // pick deterministic and independent of the caller's ordering.
            stores.sort_by(|a, b| a.id.cmp(&b.id));
            use promptdust_core::Confidence::{Likely, Unverified, Verified};
            let mut platforms: BTreeSet<String> = BTreeSet::new();
            let mut categories: BTreeSet<String> = BTreeSet::new();
            let (mut verified, mut likely, mut unverified, mut unspecified) = (0u32, 0, 0, 0);
            for &s in &stores {
                platforms.extend(s.platforms.iter().filter_map(enum_str));
                if let Some(c) = enum_str(&s.category) {
                    categories.insert(c);
                }
                // Exhaustive (no wildcard) so a new Confidence variant fails to compile here.
                match s.confidence {
                    Some(Verified) => verified += 1,
                    Some(Likely) => likely += 1,
                    Some(Unverified) => unverified += 1,
                    None => unspecified += 1,
                }
            }
            let entry = serde_json::json!({
                "tool": tool,
                // First non-null vendor by id; a tool's stores keep one vendor (test-guarded).
                "vendor": stores.iter().find_map(|s| s.vendor.clone()),
                "platforms": platforms.into_iter().collect::<Vec<_>>(),
                "categories": categories.into_iter().collect::<Vec<_>>(),
                // Non-lossy verification distribution across the tool's stores (sums to their
                // count); a single tool-level tier would bury a verified store behind a weaker
                // sibling. Per-store tiers stay in definitions[].
                "confidence_counts": {
                    "verified": verified,
                    "likely": likely,
                    "unverified": unverified,
                    "unspecified": unspecified,
                },
                "definition_ids": stores.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            });
            (tool, entry)
        })
        .collect();

    // Human-friendly, case-insensitive name order (byte order sorts "Zed" before "aichat");
    // the exact-name tiebreak keeps it deterministic.
    tools.sort_by(|(a, _), (b, _)| {
        a.to_ascii_lowercase()
            .cmp(&b.to_ascii_lowercase())
            .then_with(|| a.cmp(b))
    });
    tools.into_iter().map(|(_, entry)| entry).collect()
}

/// Serialize a metadata enum to its `catalog.json` string form (e.g. `Platform::Macos` →
/// `"macos"`), reusing the definition model's own serde representation. `Some` for any
/// string-serializing enum (all of `Platform`/`Category` today, which is why the call sites
/// can `filter_map` it); the `None` guard only trips on the unreachable non-string case.
fn enum_str<T: serde::Serialize>(v: &T) -> Option<String> {
    serde_json::to_value(v)
        .ok()
        .and_then(|x| x.as_str().map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;
    use promptdust_core::definitions::parse_str;

    // `aardvark` (lowercase) vs `Zed` (uppercase) exercise the case-insensitive order;
    // `Tool A` the platform/category unions; `Vendormix`'s two stores disagree on vendor
    // (in reverse-id fixture order) to pin the deterministic tie-break.
    const SIGS: &str = r#"[
      {"schema_version":1,"id":"aardvark-1","tool":"aardvark","platforms":["macos"],
       "paths":[{"pattern":"~/aa","match":"file"}],"category":"cache","format":"json",
       "sensitivity":"low","why":"aa"},
      {"schema_version":1,"id":"tool-a-2","tool":"Tool A","vendor":"Acme","platforms":["macos"],
       "paths":[{"pattern":"~/a2","match":"file"}],"category":"cache","format":"json",
       "sensitivity":"low","why":"a2","confidence":"verified"},
      {"schema_version":1,"id":"tool-a-1","tool":"Tool A","vendor":"Acme","platforms":["linux"],
       "paths":[{"pattern":"~/a1","match":"file"}],"category":"config_with_secrets","format":"json",
       "sensitivity":"low","why":"a1","confidence":"unverified"},
      {"schema_version":1,"id":"vendormix-2","tool":"Vendormix","vendor":"Second","platforms":["macos"],
       "paths":[{"pattern":"~/v2","match":"file"}],"category":"cache","format":"json",
       "sensitivity":"low","why":"v2"},
      {"schema_version":1,"id":"vendormix-1","tool":"Vendormix","vendor":"First","platforms":["macos"],
       "paths":[{"pattern":"~/v1","match":"file"}],"category":"cache","format":"json",
       "sensitivity":"low","why":"v1"},
      {"schema_version":1,"id":"zed-1","tool":"Zed","platforms":["macos"],
       "paths":[{"pattern":"~/z1","match":"file"}],"category":"cache","format":"json",
       "sensitivity":"low","why":"z1","confidence":"likely"}
    ]"#;

    #[test]
    fn tool_rollup_orders_case_insensitively_and_unions_stores() {
        let sigs = parse_str(SIGS).expect("fixture definitions parse");
        let refs: Vec<&Definition> = sigs.iter().collect();
        let tools = tool_rollups(&refs);

        // Case-insensitive name order: lowercase `aardvark` sorts FIRST, not after `Zed`
        // (byte order would put every uppercase-led name before it).
        let names: Vec<&str> = tools.iter().map(|t| t["tool"].as_str().unwrap()).collect();
        assert_eq!(names, ["aardvark", "Tool A", "Vendormix", "Zed"]);

        // Tool A: unioned platforms/categories, sorted definition_ids, vendor carried through.
        let a = &tools[1];
        assert_eq!(a["vendor"], "Acme");
        assert_eq!(a["platforms"], serde_json::json!(["linux", "macos"]));
        assert_eq!(
            a["categories"],
            serde_json::json!(["cache", "config_with_secrets"])
        );
        assert_eq!(
            a["definition_ids"],
            serde_json::json!(["tool-a-1", "tool-a-2"])
        );
        // Per-store facts are NOT rolled up to the tool — they live in definitions[].
        for per_store in [
            "confidence",
            "sensitivity",
            "paths",
            "why",
            "base_weight",
            "inspector",
        ] {
            assert!(
                a.get(per_store).is_none(),
                "{per_store} must not appear in a tool rollup"
            );
        }

        // aardvark has no vendor → null; Vendormix's stores disagree → the first by id wins,
        // regardless of the fixture order (the refs above are NOT pre-sorted).
        assert_eq!(tools[0]["vendor"], serde_json::Value::Null);
        assert_eq!(
            tools[2]["vendor"], "First",
            "vendor tie-break: lowest definition id wins"
        );

        // confidence_counts is the non-lossy distribution over the tool's stores (not a single
        // tier): Tool A = one verified + one unverified; aardvark's lone store has no tier
        // (unspecified); Zed's is likely. Each sums to the tool's definition_ids length.
        assert_eq!(
            a["confidence_counts"],
            serde_json::json!({"verified": 1, "likely": 0, "unverified": 1, "unspecified": 0})
        );
        assert_eq!(tools[0]["confidence_counts"]["unspecified"], 1);
        assert_eq!(tools[3]["confidence_counts"]["likely"], 1);
    }
}
