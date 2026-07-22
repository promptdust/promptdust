//! `promptdust` CLI — a thin, read-only front-end over `promptdust-core`.
//!
//! It discovers where AI tools store data on this machine and reports what amplifies
//! the exposure. It never modifies files, sends nothing off the device unless you opt
//! in, and never issues a security verdict.

mod output;
mod render;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use promptdust_core::{definitions, scan, ScanConfig};
use promptdust_telemetry::Sender;

#[derive(Parser)]
#[command(
    name = "promptdust",
    version,
    about = "Read-only local AI-data footprint scanner",
    long_about = "Discovers where AI tools store conversations, caches, and credentials \
                  on this machine, and reports what amplifies the exposure. Read-only, \
                  local-only, metadata-only. An inventory — never a verdict."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Scan this machine for AI-data artifacts (default).
    Scan(ScanArgs),
    /// Produce a redacted, path-free diagnostics bundle to paste into a bug report.
    Diagnostics(DiagnosticsArgs),
    /// Manage opt-in, anonymous usage telemetry (off by default).
    Telemetry {
        #[command(subcommand)]
        cmd: TelemetryCmd,
    },
    /// Inspect the definition database.
    Definitions {
        #[command(subcommand)]
        cmd: DefinitionsCmd,
    },
    /// Print tool and definition-database versions.
    Version,
}

#[derive(Args, Default)]
struct ScanArgs {
    /// Emit machine-readable JSON instead of a table.
    #[arg(long)]
    json: bool,
    /// Only scan these definition ids or tool names (comma-separated).
    #[arg(long, value_delimiter = ',')]
    only: Vec<String>,
    /// Skip these definition ids or tool names (comma-separated).
    #[arg(long, value_delimiter = ',')]
    exclude: Vec<String>,
    /// Restrict findings to this subtree.
    #[arg(long)]
    path: Option<PathBuf>,
    /// Skip slow shell-out probes (Time Machine, disk encryption).
    #[arg(long)]
    no_slow: bool,
    /// Byte threshold for the large_growth amplifier.
    #[arg(long)]
    large_threshold: Option<u64>,
    /// Write the report to a file (.json or .html). The report is sensitive.
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Args, Default)]
struct DiagnosticsArgs {
    /// Skip slow shell-out probes (Time Machine, disk encryption).
    #[arg(long)]
    no_slow: bool,
}

#[derive(Subcommand)]
enum TelemetryCmd {
    /// Show whether telemetry is enabled, and where the consent file lives.
    Status,
    /// Opt in to sending anonymous usage statistics.
    Enable,
    /// Opt out (the default).
    Disable,
    /// Print the exact payload that would be sent — anonymous, no paths or content.
    Preview {
        /// Skip slow shell-out probes (Time Machine, disk encryption).
        #[arg(long)]
        no_slow: bool,
    },
}

#[derive(Subcommand)]
enum DefinitionsCmd {
    /// List loaded definitions (or export the public catalog with `--json`).
    List {
        /// Emit the public catalog as JSON — the website / integrations data contract.
        #[arg(long)]
        json: bool,
    },
    /// Validate a definition file.
    Validate {
        /// Path to a definition JSON file.
        file: PathBuf,
    },
}

/// User-facing guidance printed when a release build panics (consent-based crash reporting).
/// human-panic writes a redacted report to a temp file and prints this; nothing is ever sent
/// automatically.
const CRASH_SUPPORT: &str = "Please open an issue at \
    https://github.com/promptdust/promptdust/issues and attach the report file named \
    above. It contains a technical crash backtrace, your OS, and the app version — never \
    your scanned files, their paths, or any conversation content. (`promptdust diagnostics` \
    adds more context if you want to include it.)";

/// Crash-reporter metadata for `human_panic::setup_panic!`.
fn crash_metadata() -> human_panic::Metadata {
    human_panic::Metadata::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
        .homepage("https://github.com/promptdust/promptdust")
        .support(CRASH_SUPPORT)
}

/// An env value "disables" the crash reporter when it is present and not empty/`0`
/// (the `DO_NOT_TRACK` convention).
fn env_disables(val: Option<std::ffi::OsString>) -> bool {
    val.and_then(|v| v.into_string().ok())
        .is_some_and(|v| !v.is_empty() && v != "0")
}

/// The local crash report is written by default, but suppressed by `DO_NOT_TRACK`, the
/// `PROMPTDUST_NO_CRASH_REPORT` kill-switch, or CI (never write crash artifacts in
/// automation). Nothing is ever *sent* without the user's explicit action, regardless.
fn crash_reporting_enabled() -> bool {
    !env_disables(std::env::var_os("DO_NOT_TRACK"))
        && !env_disables(std::env::var_os("PROMPTDUST_NO_CRASH_REPORT"))
        && !env_disables(std::env::var_os("CI"))
}

fn main() -> ExitCode {
    // Crash reporting: on a *release* panic, write a redacted report to a temp file and tell
    // the user how to share it — opt-out via DO_NOT_TRACK / the kill-switch / CI. No-op in
    // debug; never auto-sends.
    if crash_reporting_enabled() {
        human_panic::setup_panic!(crash_metadata());
    }
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Scan(ScanArgs::default())) {
        Command::Scan(args) => run_scan(&args),
        Command::Diagnostics(args) => run_diagnostics(&args),
        Command::Telemetry { cmd } => run_telemetry(&cmd),
        Command::Definitions { cmd } => run_definitions(&cmd),
        Command::Version => {
            let loaded = definitions::load_bundled();
            println!(
                "PromptDust {} (definitions DB {})",
                env!("CARGO_PKG_VERSION"),
                loaded.db_version
            );
            ExitCode::SUCCESS
        }
    }
}

fn run_scan(args: &ScanArgs) -> ExitCode {
    let Some(mut cfg) = ScanConfig::detect() else {
        eprintln!("error: could not determine your home directory");
        return ExitCode::from(2);
    };
    cfg.only = args.only.clone();
    cfg.exclude = args.exclude.clone();
    cfg.no_slow = args.no_slow;
    if let Some(t) = args.large_threshold {
        cfg.large_threshold = t;
    }
    if let Some(p) = &args.path {
        cfg.root_override = Some(p.clone());
    }
    // Supply "now" so the clockless core can score recency + the dual number.
    cfg.now_epoch = Some(time::OffsetDateTime::now_utc().unix_timestamp());

    let home = cfg.home.display().to_string();
    let started = std::time::Instant::now();
    let result = scan(&cfg);
    let scan_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    if let Some(out) = &args.output {
        eprintln!(
            "warning: this report maps where sensitive AI data lives on this machine — store it carefully."
        );
        let content = if is_html(out) {
            html_report(&render::human(&result, &home))
        } else {
            output::to_json(&result)
        };
        if let Err(e) = std::fs::write(out, content) {
            eprintln!("error: could not write {}: {e}", out.display());
            return ExitCode::from(2);
        }
    }

    if args.json {
        // stdout carries ONLY the JSON document (warnings go to stderr).
        println!("{}", output::to_json(&result));
    } else {
        print!("{}", render::human(&result, &home));
    }

    maybe_run_telemetry(&result, args, scan_ms);
    ExitCode::SUCCESS
}

/// Active feature-flag names for the telemetry payload — names only, never values.
fn feature_flags(no_slow: bool) -> Vec<String> {
    let mut flags = Vec::new();
    if no_slow {
        flags.push("no_slow".to_string());
    }
    flags
}

/// After a scan: show the one-time first-run notice (interactive, not suppressed), and — if
/// the user has opted in and telemetry is active — build and "send" the anonymous payload.
/// The sender is a no-op today (no backend yet); the client is otherwise fully wired.
fn maybe_run_telemetry(result: &promptdust_core::ScanResult, args: &ScanArgs, scan_ms: u64) {
    use std::io::IsTerminal;

    let Some(config_dir) = promptdust_telemetry::config_dir() else {
        return;
    };
    let mut consent = promptdust_telemetry::Consent::load(&config_dir);

    // One-time, non-blocking first-run notice — only on an interactive terminal, and never
    // when the environment suppresses telemetry.
    if !consent.notified
        && !promptdust_telemetry::suppressed_by_env()
        && std::io::stderr().is_terminal()
    {
        eprintln!(
            "note: PromptDust can send anonymous usage stats to help improve it — off by \
             default, no file paths or content. Run `promptdust telemetry enable` to opt in \
             (`telemetry status` for details). You won't see this again."
        );
        consent.notified = true;
        let _ = consent.save(&config_dir);
    }

    if promptdust_telemetry::is_active(&consent) {
        let payload = build_payload(result, scan_ms, args.no_slow);
        // Stubbed sender: there is no backend yet, so this is a no-op.
        let _ = promptdust_telemetry::NoopSender.send(&payload.to_json_pretty());
    }
}

/// Assemble the anonymous telemetry payload for the current run + host.
fn build_payload(
    result: &promptdust_core::ScanResult,
    scan_ms: u64,
    no_slow: bool,
) -> promptdust_telemetry::Payload {
    promptdust_telemetry::Payload::new(
        result,
        env!("CARGO_PKG_VERSION").to_string(),
        std::env::consts::OS.to_string(),
        std::env::consts::ARCH.to_string(),
        Some(scan_ms),
        feature_flags(no_slow),
    )
}

fn run_telemetry(cmd: &TelemetryCmd) -> ExitCode {
    let Some(config_dir) = promptdust_telemetry::config_dir() else {
        eprintln!("error: could not determine a config directory");
        return ExitCode::from(2);
    };
    match cmd {
        TelemetryCmd::Status => {
            let consent = promptdust_telemetry::Consent::load(&config_dir);
            println!(
                "telemetry: {}",
                if consent.is_enabled() {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            if promptdust_telemetry::suppressed_by_env() {
                println!(
                    "  (forced off by the environment: DO_NOT_TRACK / PROMPTDUST_TELEMETRY / CI)"
                );
            }
            println!(
                "consent file: {}",
                config_dir.join("consent.json").display()
            );
            println!(
                "It is anonymous and opt-in — a per-run random id, no file paths, no content. \
                 `promptdust telemetry preview` shows the exact payload."
            );
            ExitCode::SUCCESS
        }
        TelemetryCmd::Enable => {
            set_consent(&config_dir, promptdust_telemetry::TelemetryState::Enabled)
        }
        TelemetryCmd::Disable => {
            set_consent(&config_dir, promptdust_telemetry::TelemetryState::Disabled)
        }
        TelemetryCmd::Preview { no_slow } => {
            let Some(mut cfg) = ScanConfig::detect() else {
                eprintln!("error: could not determine your home directory");
                return ExitCode::from(2);
            };
            cfg.no_slow = *no_slow;
            let started = std::time::Instant::now();
            let result = scan(&cfg);
            let scan_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            eprintln!(
                "note: this is the exact payload telemetry would send when enabled — anonymous, \
                 no file paths or content. `preview` sends nothing."
            );
            println!(
                "{}",
                build_payload(&result, scan_ms, *no_slow).to_json_pretty()
            );
            ExitCode::SUCCESS
        }
    }
}

fn set_consent(
    config_dir: &std::path::Path,
    state: promptdust_telemetry::TelemetryState,
) -> ExitCode {
    let mut consent = promptdust_telemetry::Consent::load(config_dir);
    consent.state = state;
    consent.notified = true;
    if let Err(e) = consent.save(config_dir) {
        eprintln!("error: could not write the consent file: {e}");
        return ExitCode::from(2);
    }
    println!(
        "telemetry {}.",
        if state == promptdust_telemetry::TelemetryState::Enabled {
            "enabled — thank you"
        } else {
            "disabled"
        }
    );
    ExitCode::SUCCESS
}

fn run_diagnostics(args: &DiagnosticsArgs) -> ExitCode {
    let Some(mut cfg) = ScanConfig::detect() else {
        eprintln!("error: could not determine your home directory");
        return ExitCode::from(2);
    };
    cfg.no_slow = args.no_slow;
    // No `now_epoch`: the bundle is count-only (`RedactedSummary`), so the dual-score pass
    // the clockless core would run is pure waste here.
    let started = std::time::Instant::now();
    let result = scan(&cfg);
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    // The bundle is redacted (no paths, no content), so — unlike an exported report — it is
    // safe to share; still prompt the user to eyeball it first.
    eprintln!(
        "note: this bundle contains only counts, versions, and OS info — no file paths or \
         conversation content. Review it, then paste it into a bug report."
    );
    println!("{}", output::diagnostics_json(&result, elapsed_ms));
    ExitCode::SUCCESS
}

fn run_definitions(cmd: &DefinitionsCmd) -> ExitCode {
    match cmd {
        DefinitionsCmd::List { json } => {
            let mut loaded = definitions::load_bundled();
            if let Some(dir) = promptdust_core::platform::user_definitions_dir() {
                definitions::load_user_dir(&dir, &mut loaded);
            }
            if *json {
                println!("{}", output::catalog_json(&loaded));
                return ExitCode::SUCCESS;
            }
            println!("definitions DB {}", loaded.db_version);
            let mut sigs = loaded.definitions;
            sigs.sort_by(|a, b| a.id.cmp(&b.id));
            for s in &sigs {
                let plats: Vec<&str> = s
                    .platforms
                    .iter()
                    .map(|p| match p {
                        promptdust_core::Platform::Macos => "macos",
                        promptdust_core::Platform::Linux => "linux",
                        promptdust_core::Platform::Windows => "windows",
                    })
                    .collect();
                let conf = s.confidence.map_or("-", |c| match c {
                    promptdust_core::Confidence::Verified => "verified",
                    promptdust_core::Confidence::Likely => "likely",
                    promptdust_core::Confidence::Unverified => "unverified",
                });
                println!(
                    "  {:<28} {:<16} [{}] {}",
                    s.id,
                    s.tool,
                    plats.join(","),
                    conf
                );
            }
            println!("({} definitions)", sigs.len());
            ExitCode::SUCCESS
        }
        DefinitionsCmd::Validate { file } => match std::fs::read_to_string(file) {
            Ok(contents) => match definitions::parse_str(&contents) {
                Ok(sigs) => {
                    println!(
                        "OK: {} definition(s) valid in {}",
                        sigs.len(),
                        file.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("invalid: {e}");
                    ExitCode::from(1)
                }
            },
            Err(e) => {
                eprintln!("error: could not read {}: {e}", file.display());
                ExitCode::from(2)
            }
        },
    }
}

fn is_html(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
}

/// A minimal, self-contained HTML wrapper around the plain-text report. (A richer
/// styled report is planned for later; this stays dependency-free and offline.)
fn html_report(human: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>PromptDust report</title>\
<style>body{{font-family:system-ui,sans-serif;margin:2rem;max-width:70rem;color:#222}}\
pre{{white-space:pre-wrap;background:#f6f6f6;padding:1rem;border-radius:8px;overflow-x:auto}}\
@media(prefers-color-scheme:dark){{body{{background:#111;color:#eee}}pre{{background:#1c1c1c}}}}</style>\
</head><body><h1>PromptDust report</h1>\
<p>Read-only inventory of AI-data on this machine. This file maps where sensitive \
data lives — store it carefully.</p><pre>{}</pre></body></html>\n",
        escape_html(human)
    )
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_support_message_is_consent_based() {
        // The message must guide *manual* sharing, disclaim content, and never imply auto-send.
        assert!(CRASH_SUPPORT.contains("open an issue"));
        let lower = CRASH_SUPPORT.to_lowercase();
        assert!(lower.contains("conversation content") && lower.contains("scanned files"));
        assert!(!lower.contains("automatically"));
    }

    #[test]
    fn crash_reporting_respects_do_not_track_and_kill_switch() {
        // Pure gate logic (no process-env mutation): the local report is on by default, but
        // DO_NOT_TRACK / the kill-switch / CI each suppress it, per DO_NOT_TRACK semantics
        // (present + not empty/`0`).
        assert!(!env_disables(None), "unset → enabled");
        assert!(!env_disables(Some("".into())), "empty → enabled");
        assert!(!env_disables(Some("0".into())), "\"0\" → enabled");
        assert!(env_disables(Some("1".into())), "set → disabled");
        assert!(env_disables(Some("true".into())), "truthy → disabled");
        // The hook metadata builds (the release-only hook itself can't run in a debug test).
        let _ = crash_metadata();
    }
}
