# Desktop e2e tests

BDD (Gherkin) screen-automation for the desktop app, in two layers. Together with the unit
tests (`desktop/ui/*.test.mjs`, `node --test`) and the Rust backend tests, they form the
testing pyramid.

## Layer 1 — UI workflows (`./`, Playwright + playwright-bdd)

Drives the **real** `desktop/ui/` in a real **WebKit** context (the macOS WKWebView family)
against a seedable in-JS **fake backend** (`support/fakeBackend.js`). Covers every workflow —
scan, inbox history + persistence, findings filter/expand, detail + reveal + share, feedback
overlays, export. Deterministic, fast, and it **runs on macOS** (unlike the real webview,
which has no macOS WebDriver).

```sh
cd desktop/e2e
npm ci
npx playwright install webkit   # one-time
npm run e2e                      # or: npm run e2e:headed / npm run e2e:ui
```

The fake backend persists to `localStorage`, so scan history and pin/flag survive a page
reload — modelling the real on-disk store, so "relaunch" scenarios work.

## Layer 2 — real-app smoke (`./app-smoke`, WebdriverIO + tauri-driver)

Drives the **real built app** (real Rust backend, real Tauri IPC, real on-disk store) over a
synthetic home. Proves the integration the fake-backend layer can't. **Linux/Windows only** —
Apple ships no WebDriver for WKWebView, so this is a CI layer, not a local-on-Mac one.

It points the app at a throwaway environment so the real scan is deterministic and writes
nothing to your machine:

- `PROMPTDUST_HOME` → a synthetic home the scan detects (`fixture.mjs` builds it).
- `PROMPTDUST_CONFIG_DIR` → a throwaway dir for the Inbox store + consent.
- `PROMPTDUST_TELEMETRY=0` → sharing forced off.

Runs in CI (`.github/workflows/e2e.yml`, `app-smoke` job) on Ubuntu with WebKitGTK + xvfb.

## CI

`.github/workflows/e2e.yml` runs both layers on PRs to `main` and on manual dispatch.
`node_modules`, browsers, and `test-results/` are gitignored.
