#!/usr/bin/env bash
# INV-2: the core scanning engine MUST NOT link any networking client crate.
# This is a static guard complementing the runtime no-network test.
set -euo pipefail

FORBIDDEN='reqwest|hyper|h2|ureq|curl|isahc|surf|attohttpc|tungstenite|tokio-tungstenite|websocket|native-tls|openssl|rustls|hyper-tls'

# Only normal (non-dev) dependencies of the core crate are relevant.
tree="$(cargo tree -p promptdust-core --edges normal --prefix none 2>/dev/null | sed 's/[[:space:]].*//' | sort -u)"
hits="$(printf '%s\n' "$tree" | grep -Ei "^(${FORBIDDEN})\$" || true)"

if [ -n "$hits" ]; then
  echo "INV-2 VIOLATION: networking/TLS crate(s) linked into promptdust-core:" >&2
  printf '  - %s\n' $hits >&2
  echo "The core scan path must make zero network calls." >&2
  exit 1
fi

echo "OK: no networking crates in promptdust-core dependency tree."
