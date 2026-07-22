#!/usr/bin/env bash
# INV-5 (no default/undisclosed telemetry), core half: the scanning engine MUST NOT link
# any usage-analytics / product-telemetry / crash-reporting SDK. Opt-in, consent-gated
# telemetry is permitted only in the front-ends (cli/desktop), never in `promptdust-core`
# (ADR-021). This is the static complement to the INV-2 no-network guard.
#
# Scope (deliberately narrow): this proves the *categorical* core guarantee — no analytics
# crate is linked into the engine. The front-end guarantees (off by default, explicit opt-in
# consent, DO_NOT_TRACK, metadata-only payload) are enforced elsewhere: consent lives in the
# front-end, and payload path/content-freedom is guarded by the INV-3 canary over
# `RedactedSummary` (core/tests/invariants.rs::redacted_summary_is_path_and_canary_free).
# Network transports used by observability SDKs (opentelemetry, datadog, …) are already
# blocked for the core by check_no_network.sh, so they are not duplicated here.
set -euo pipefail

# The analytics / crash-reporting / observability SDKs a Rust/Tauri app would realistically
# link as a crate (aptabase is the common Tauri telemetry plugin; the sentry.*/posthog.*/
# opentelemetry.* branches cover their sub-crates). The bare opentelemetry/datadog *API*
# crates carry no networking dependency of their own, so — unlike their exporters — the INV-2
# network guard would NOT catch them; they are listed here explicitly. Service-only names
# (mixpanel, amplitude, segment, …) have no Rust crate and are reached over HTTP, so an
# in-core integration would trip the INV-2 guard first — those are not listed.
FORBIDDEN='sentry.*|posthog.*|aptabase|tauri-plugin-aptabase|bugsnag|rollbar|opentelemetry.*|tracing-opentelemetry|datadog.*|newrelic|libhoney'

# Only normal (non-dev) dependencies of the core crate are relevant.
tree="$(cargo tree -p promptdust-core --edges normal --prefix none 2>/dev/null | sed 's/[[:space:]].*//' | sort -u)"
hits="$(printf '%s\n' "$tree" | grep -Ei "^(${FORBIDDEN})\$" || true)"

if [ -n "$hits" ]; then
  echo "INV-5 VIOLATION: analytics/telemetry crate(s) linked into promptdust-core:" >&2
  printf '  - %s\n' $hits >&2
  echo "The engine must carry no telemetry; opt-in telemetry lives only in the front-ends (ADR-021)." >&2
  exit 1
fi

echo "OK: no analytics/telemetry crates in promptdust-core dependency tree."
