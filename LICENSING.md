# Licensing & open-core boundary

PromptDust is licensed under the **Apache License 2.0** (see [`LICENSE`](LICENSE) and
[`NOTICE`](NOTICE)). The license choice is recorded as **ADR-009** and the open-core split
as **ADR-016**.

## What is open (forever)

The **engine** — `promptdust-core`, `promptdust-cli`, and `promptdust-desktop` — and the
bundled **definitions database** (`core/definitions/`) are open-source under Apache-2.0,
permanently. This is the code that runs on and inspects the user's machine; trust in a
privacy tool requires that code to be auditable.

## What may be private (later)

Paid layers may be closed-source and live *outside* the open engine, as separate crates
or a separate private repository. Two distinct tiers are anticipated:

- **PromptDust Pro**: paid features for individuals, delivered as an in-app purchase or
  subscription, built on top of the open engine.
- **Enterprise / governance**: an organization-scale offering (managed/forensic
  orchestration, fleet aggregation, policy-as-code, advanced classifiers, role-separated
  escalation).

**None of it ships today; there is zero commercial code in this repo.**

## The load-bearing rule

Any private component **depends on** the open engine as a consumer; it must never link
**into** `promptdust-core`. This is not just tidiness — it is what keeps the local-only
guarantee (INV-2) enforceable: the `check_no_network.sh` guard walks only the
`promptdust-core` dependency tree, so any networking, telemetry, or cloud capability must
live in the layer *above* core, never inside it. Keeping the commercial surface outside
core means the no-network invariant on the scan path can be proven mechanically, forever.
