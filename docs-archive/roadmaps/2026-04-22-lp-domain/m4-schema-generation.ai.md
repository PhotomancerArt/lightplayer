# Milestone 4: Schema generation tooling + immutability gates

## Goal

Build the schema-generation tooling: `lp-cli schema generate`, the
committed `schemas/v1/<kind>.schema.json` files, and the CI gates
that enforce drift-free, version-immutable schemas.

The `JsonSchema` derives themselves are already in place from M2
(Q6), so this milestone is **pure tooling work** — no derive sweep
needed, no risk of schemars choking on recursive types (verified in
M2). Codegen, CLI, schema-diff, three CI gates.

## Suggested plan name

`lp-domain-m4`

## Scope

**In scope:**

- `lp-domain/lp-domain/src/schema/gen.rs`:
  - `generate_all() -> HashMap<&'static str, serde_json::Value>` →
    one entry per Visual kind, each mapping to its current JSON
    Schema (via `schemars::schema_for!`).
  - Helper to write a kind's schema to disk at the canonical path.
  - Behind the `schema-gen` feature flag (host-side only;
    int-only-firmware builds don't need codegen).

- `lp-cli schema generate` command:
  - Writes `lp-domain/lp-domain/schemas/v1/<kind>.schema.json` for
    each kind (note **`v1/` prefix**, matching M3's `examples/v1/`
    layout, Q5).
  - On a version bump (detected by reading `CURRENT_VERSION` and
    comparing to existing latest schema version), writes the new
    schema at `schemas/v<N>/<kind>.schema.json`. The previous
    `v<N-1>/<kind>.schema.json` becomes the immutable historical
    record (M5's `--bump` workflow snapshots it; M4 just lays down
    the directory convention). For initial v1, every schema lands
    fresh.
  - `--check` flag: regenerate to a tempdir, compare against
    committed; non-zero exit if they differ. Used by CI.

- **Initial schemas committed** to
  `lp-domain/lp-domain/schemas/v1/<kind>.schema.json` (six files,
  all v1).

- **CI gate 1 (drift)**: `lp-cli schema generate` then `git diff
  --exit-code`. Catches "I changed the model but forgot to
  regenerate."

- **CI gate 2 (immutability)**: hash check on `schemas/v<N>/` for
  every `N < current` — every file's content hash must match a
  committed manifest; any drift = fail. Catches "I edited a frozen
  schema by mistake."

- **CI gate 3 (additive-vs-breaking)**: schema-diff utility (basic
  implementation) compares current `schemas/v<N>/<kind>.schema.json`
  against immutable `schemas/v<N-1>/<kind>.schema.json` (when one
  exists). Flags non-additive changes (removed fields, narrowed
  types, new required fields, etc.). Non-additive diff without a
  `CURRENT_VERSION` bump = fail. For v1's initial commit this is a
  no-op (no v0); the gate becomes meaningful once v2 schemas land.

**Out of scope:**

- The `schemars` derive sweep — done in M2 (Q6).
- Migration framework itself (M5).
- Round-trip / forward-compat tests (M5).
- `examples/v1/<kind>/history/` (M5).
- Sophisticated JSON Schema diff semantics (we ship a basic
  additive-only check; refine later if needed).
- The `--bump` workflow (M5).

## Key decisions

- **Schemars derives are already in place from M2.** This milestone
  is tooling only. If schemars choked on a type, M2 would have
  caught it (Q6). M4 design becomes much simpler as a result.
- **Schemas are committed**, not generated at build time. CI catches
  drift via `git diff` (gate 1).
- **Schemas live under `schemas/v<N>/<kind>.schema.json`.** Versioned
  directory matches `examples/v<N>/...` layout from M3 (Q5).
  Symmetric, easy to grep (`schemas/v1/` is "everything v1
  schemas"), and the M5 `--bump` workflow is mechanical (copy
  `v<N>/` to a fresh `v<N+1>/` directory or similar).
- **Schema-diff is a custom utility**, not a vendored crate, to
  keep the dependency footprint small. v0 implements only the rules
  we need (added optional field = additive; removed field =
  breaking; type change = breaking; required-vs-optional change =
  breaking). Non-trivial cases get flagged for human review.
- **`schemars` codegen lives behind `schema-gen` feature** so
  on-device builds don't pull it. The derives themselves remain
  always-on (no feature gate, per Q6) — this gate is for
  `gen.rs` and any std-only generator code.

## Deliverables

- `lp-domain/lp-domain/src/schema/gen.rs` with `generate_all` and
  per-kind helpers.
- `lp-cli/src/commands/schema/` with `generate` subcommand
  (`--check`, default writes).
- `lp-domain/lp-domain/schemas/v1/<kind>.schema.json` (six files,
  committed).
- Schema-diff utility (can live in `lp-domain` under a `schema-gen`
  module, or as a small helper in `lp-cli`).
- CI workflow updates (or `justfile` targets) for the three gates.
- `lp-domain` README section documenting the schema discipline and
  the `schemas/v<N>/` layout.

## Dependencies

- M3 complete (artifact types exist with `JsonSchema` derives from
  M2 + serde from M3; example corpus at `examples/v1/` for the
  initial schema generation pass).

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: Three distinct components (codegen module, CLI
subcommand, custom schema-diff utility) plus three CI gates.
Parallelisable: codegen and schema-diff can proceed independently
after the layout convention is locked.

Suggested phase shape (decided during `/plan`, sketched here):

- Phase A: `schema/gen.rs` + commit the initial six v1 schemas at
  `schemas/v1/<kind>.schema.json` — solo
- Phase B: `lp-cli schema generate` command (no `--check` yet) —
  depends on A
- Phase C: schema-diff utility + `--check` CLI flag — depends on
  A, parallel with B
- Phase D: CI wiring (gates 1, 2, 3) + README — depends on B+C
- Phase E: end-to-end verify — final

Estimated size: ~150 LOC `gen.rs` + ~150 LOC CLI + ~400–600 LOC
schema-diff utility (the bulk of the milestone) + CI yaml updates.
Smaller than the original M4 estimate because the derive sweep is
already done.

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?
