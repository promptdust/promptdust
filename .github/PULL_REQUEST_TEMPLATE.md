<!-- Heads up: outside code contributions are not accepted (see CONTRIBUTING.md). To add a
     tool PromptDust should map, or to report a bug, please open an issue instead. This
     template is for maintainer pull requests. -->

## What this changes

## Checklist

- [ ] I read the four principles (read-only, local-only, metadata-only, inventory-not-a-verdict).
- [ ] **No real conversation data, secrets, or personal data** is included anywhere.
- [ ] Tests added/updated (synthetic fixtures; real tools over mocks where practical).
- [ ] Considered impact on the invariants (INV-1..6); invariant tests still green.
- [ ] `make lint test` (or `just lint test`) passes locally.
- [ ] Definitions (if any) validate: `python3 .github/scripts/validate_signatures.py`.
- [ ] Docs updated if behavior or the output contract changed.

## Notes for reviewers
