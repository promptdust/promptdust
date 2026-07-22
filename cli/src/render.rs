//! Human-readable rendering of a scan. Deliberately an *inventory*: it states what
//! was found and why it might matter, and never issues a security verdict (FR-5).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use promptdust_core::{Finding, ScanResult};

/// Render a scan result as a plain-text report.
#[must_use]
pub fn human(result: &ScanResult, home: &str) -> String {
    let mut out = String::new();

    let _ = writeln!(
        out,
        "PromptDust {} — definitions DB {}",
        env!("CARGO_PKG_VERSION"),
        result.definition_db_version
    );
    let _ = writeln!(out, "Scanned: {home}");
    let _ = writeln!(out, "Disk encryption: {}", disk_str(result.disk_encryption));
    let _ = writeln!(out, "Mode: {}", result.mode.as_str());
    // The endpoint dual score (present once the front-end has scored the result). Two honest
    // numbers: Exposure is magnitude, Assurance is how much to trust it — read together.
    if let (Some(exp), Some(assur)) = (result.exposure.as_ref(), result.assurance.as_ref()) {
        let _ = writeln!(out, "Exposure: {}/100 ({})", exp.score, exp.band.as_str());
        let _ = writeln!(
            out,
            "Assurance: {}/100 ({})",
            assur.score,
            assur.band.as_str()
        );
        if let Some(interp) = result.interpretation {
            let _ = writeln!(out, "{interp}");
        }
    }
    let _ = writeln!(out);

    let s = &result.summary;
    if s.total_findings == 0 {
        let _ = writeln!(
            out,
            "No AI-data artifacts matched the current definitions on this machine."
        );
        let _ = writeln!(
            out,
            "(This is coverage of known tools only — not a statement about everything present.)"
        );
        render_warnings(&mut out, result);
        return out;
    }

    let _ = writeln!(
        out,
        "Found {} artifact(s) across {} tool(s), {} total.",
        s.total_findings,
        s.by_tool.len(),
        human_size(s.total_bytes)
    );
    let _ = writeln!(out, "By level: {}", exposure_line(&s.by_exposure));
    let _ = writeln!(out);

    // Group by tool, order groups by their most-exposed finding.
    let mut by_tool: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for f in &result.findings {
        by_tool.entry(f.tool.as_str()).or_default().push(f);
    }
    let mut groups: Vec<(&str, Vec<&Finding>)> = by_tool.into_iter().collect();
    for (_, findings) in &mut groups {
        findings.sort_by_key(|f| std::cmp::Reverse(f.exposure_level));
    }
    groups.sort_by_key(|g| std::cmp::Reverse(max_exposure(&g.1)));

    for (tool, findings) in groups {
        let _ = writeln!(out, "▸ {} ({})", tool, findings.len());
        for f in findings {
            render_finding(&mut out, f);
        }
        let _ = writeln!(out);
    }

    render_warnings(&mut out, result);
    out
}

fn render_finding(out: &mut String, f: &Finding) {
    let _ = writeln!(
        out,
        "  [{}] {}",
        f.exposure_level.as_str().to_uppercase(),
        f.path.display()
    );

    let mut facts = vec![human_size(f.size_bytes)];
    if f.file_count > 1 {
        facts.push(format!("{} files", f.file_count));
    }
    if let Some(insp) = &f.inspection {
        if let Some(n) = insp.line_count {
            facts.push(format!("{n} lines"));
        }
        if let Some(n) = insp.row_count {
            facts.push(format!("{n} rows"));
        }
    }
    facts.push(format!("evidence: {}", f.evidence_class.as_str()));
    let _ = writeln!(out, "     {}", facts.join(" · "));

    if !f.amplifiers.is_empty() {
        let amps: Vec<&str> = f.amplifiers.iter().map(|a| a.as_str()).collect();
        let _ = writeln!(out, "     amplifiers: {}", amps.join(", "));
    }
    let _ = writeln!(out, "     why: {}", f.why);
    for g in &f.guidance {
        let _ = writeln!(out, "       - {g}");
    }
}

fn render_warnings(out: &mut String, result: &ScanResult) {
    if result.warnings.is_empty() {
        return;
    }
    let _ = writeln!(out, "Warnings ({}):", result.warnings.len());
    for w in &result.warnings {
        match &w.path {
            Some(p) => {
                let _ = writeln!(out, "  - {}: {}", p.display(), w.reason);
            }
            None => {
                let _ = writeln!(out, "  - {}", w.reason);
            }
        }
    }
}

fn exposure_line(by_exposure: &BTreeMap<String, usize>) -> String {
    // Present most-severe first.
    let order = ["critical", "high", "medium", "low", "info"];
    let parts: Vec<String> = order
        .iter()
        .filter_map(|k| by_exposure.get(*k).map(|n| format!("{n} {k}")))
        .collect();
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(", ")
    }
}

fn max_exposure(findings: &[&Finding]) -> promptdust_core::ExposureLevel {
    findings
        .iter()
        .map(|f| f.exposure_level)
        .max()
        .unwrap_or(promptdust_core::ExposureLevel::Info)
}

fn disk_str(d: promptdust_core::DiskEncryption) -> &'static str {
    use promptdust_core::DiskEncryption::{Off, On, Unknown};
    match d {
        On => "on",
        Off => "off",
        Unknown => "unknown",
    }
}

/// Human-readable byte size.
#[must_use]
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_scales() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn exposure_line_orders_severe_first() {
        let mut m = BTreeMap::new();
        m.insert("low".to_string(), 2);
        m.insert("critical".to_string(), 1);
        assert_eq!(exposure_line(&m), "1 critical, 2 low");
    }

    use promptdust_core::{
        Amplifier, Category, DiskEncryption, EvidenceClass, ExposureLevel, Format, Inspection,
        Mode, ScanWarning, Sensitivity, Summary,
    };

    fn finding(tool: &str, level: ExposureLevel, is_dir: bool, rows: Option<u64>) -> Finding {
        Finding {
            definition_id: "sig".into(),
            tool: tool.into(),
            category: Category::Transcript,
            format: Format::Jsonl,
            path: std::path::PathBuf::from("/x/y.jsonl"),
            size_bytes: 2048,
            file_count: if is_dir { 3 } else { 1 },
            modified_epoch_secs: Some(0),
            inspection: rows.map(|r| Inspection {
                line_count: None,
                row_count: Some(r),
            }),
            amplifiers: vec![Amplifier::CloudSync],
            amplifier_detail: serde_json::json!({ "cloud_sync": { "provider": "Dropbox" } }),
            sensitivity: Sensitivity::High,
            evidence_class: EvidenceClass::Presence,
            base_weight: None,
            exposure_level: level,
            computed: None,
            why: "why text".into(),
            guidance: vec!["do this".into()],
            confidence: None,
        }
    }

    #[test]
    fn human_renders_findings_grouped_by_exposure_with_warnings() {
        let mut by_tool = BTreeMap::new();
        by_tool.insert("Cursor".to_string(), 1);
        by_tool.insert("Claude Code".to_string(), 1);
        let mut by_exposure = BTreeMap::new();
        by_exposure.insert("critical".to_string(), 1);
        by_exposure.insert("medium".to_string(), 1);
        let summary = Summary {
            total_findings: 2,
            total_bytes: 4096,
            by_tool,
            by_exposure,
        };
        let mut result = ScanResult {
            schema_version: 1,
            definition_db_version: "2026.07.0".into(),
            disk_encryption: DiskEncryption::On,
            mode: Mode::Inventory,
            exposure: None,
            assurance: None,
            interpretation: None,
            findings: vec![
                finding("Cursor", ExposureLevel::Medium, true, Some(10)),
                finding("Claude Code", ExposureLevel::Critical, false, None),
            ],
            warnings: vec![ScanWarning::new_path("/bad", "permission denied")],
            summary,
        };
        result.score(1_000_000_000, |_| Some(EvidenceClass::Content));
        let out = human(&result, "/home/u");
        assert!(out.contains("Claude Code") && out.contains("Cursor"));
        assert!(out.contains("3 files"));
        assert!(out.contains("10 rows"));
        assert!(out.contains("amplifiers: cloud_sync"));
        assert!(out.contains("Warnings (1)"));
        assert!(out.contains("Disk encryption: on"));
        assert!(out.contains("Mode: inventory"));
        // The endpoint dual score is surfaced with both numbers + a plain-English reading.
        assert!(out.contains("Exposure: ") && out.contains("/100 ("));
        assert!(out.contains("Assurance: "));
        assert!(
            !out.contains("clean"),
            "interpretation must not use a 'clean' verdict"
        );
        assert!(out.contains("evidence: presence"));
        // Highest-exposure tool group is rendered first.
        assert!(out.find("Claude Code").unwrap() < out.find("Cursor").unwrap());
    }

    #[test]
    fn human_handles_empty_result_and_disk_off() {
        let result = ScanResult {
            schema_version: 1,
            definition_db_version: "v".into(),
            disk_encryption: DiskEncryption::Off,
            mode: Mode::Inventory,
            exposure: None,
            assurance: None,
            interpretation: None,
            findings: vec![],
            warnings: vec![],
            summary: Summary::default(),
        };
        let out = human(&result, "/h");
        assert!(out.contains("No AI-data artifacts matched"));
        assert!(out.contains("Disk encryption: off"));
    }
}
