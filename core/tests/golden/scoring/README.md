# Endpoint Exposure — golden scoring fixtures

The endpoint Exposure score (`score::exposure_of` / `score::score_endpoint`) is a Rust port
of the PromptDust scoring doctrine — `scoring_model.yaml` (§8 of the org's private
`01_Charter_and_Doctrine`, per the YAML's own `implements:` header) and its authoritative
reference implementation `score_reference.py`. Both are **vendored here** for provenance and
regeneration; the golden numbers are transcribed from them into `scenarios.json`, which
`core/tests/scoring_golden.rs` asserts the Rust scorer reproduces. (Nothing auto-checks
`scoring_model.yaml` against `score::policy` — keep them in sync by hand when tuning.)

| File | Role |
|------|------|
| `scoring_model.yaml` | The policy: multipliers, recency decay, normalizer, cap, bands. `score::policy` is a faithful transcription of it. |
| `score_reference.py` | The **authoritative** reference implementation + three worked scenarios (A/B/C). The source of the golden numbers. |
| `scenarios.json` | Those three scenarios as PromptDust-native fixtures, each with the `expected_exposure` the Rust scorer must reproduce. |

## Regenerating the golden numbers

The numbers in `scenarios.json` come from `score_reference.py`. To re-derive them:

```sh
cd core/tests/golden/scoring && python3 score_reference.py   # needs PyYAML
```

Read the `EXPOSURE = N (band)` line printed for each scenario; that `N` is
`expected_exposure`. A change to any number means the scoring **model** changed — update
`score::policy` and these fixtures together, deliberately. Never edit a golden number just
to make a test pass.

## Naming note

`scenarios.json` uses PromptDust's `SensitivityType` names. The doctrine's `source_code`
(content-factor weight 1.15) is our `source`; the other confirmed types match by name.

Scope: these fixtures cover both endpoint scores — **Exposure** (from `findings`) and
**Assurance** (from `coverage_gaps` / `evasion_signals` / `corroborated_findings`). Wiring
either score to a real scan and surfacing it in the output is a separate sub-issue.
