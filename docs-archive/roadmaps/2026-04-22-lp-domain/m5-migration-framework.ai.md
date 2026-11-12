# Milestone 5: Migration framework + lp-cli migrate + compat CI gates

## Goal

Stand up the per-kind migration registry, the `lp-cli migrate`
tooling, the `examples/v<N>/<kind>/history/` immutable archive
convention, and the forward/backward-compat CI gates.

Prove the framework works end-to-end via a **synthetic `v0_5 → v1`
smoke test** (Q5) — a deliberately-built fake old-version corpus
that exercises every migration primitive (field rename, struct
reshape, default change, optional → required, array length change).
The smoke test stays in CI as the framework's regression guard.

This is the milestone that makes schema evolution safe and routine.

## Suggested plan name

`lp-domain-m5`

## Scope

**In scope:**

- Migration registry in `lp-domain/lp-domain/src/migration/`:
  - `Migration` trait (already declared in M2): `KIND`, `FROM: u32`,
    `migrate(toml::Value) -> Result<toml::Value>`.
  - `MigrationRegistry` keyed by `(kind, from_version)`.
  - `migrate_to_current(kind, value) -> Result<toml::Value>`:
    iteratively apply registered migrations until reaching
    `CURRENT_VERSION`.
  - `load_migrated<T: Artifact>(value) -> Result<T>`: full pipeline:
    read `schema_version`, run migrations, deserialize via serde.
- All artifact loaders go through `load_migrated`.

- `lp-cli migrate <path>`:
  - Accepts a single file or a directory.
  - For each file: detect kind from filename suffix
    (`*.<kind>.toml`), read `schema_version`, run migrations to
    current, write back in place.
  - `--dry-run` prints the diff.
  - `--all` walks `lp-domain/lp-domain/examples/v<latest>/` and
    migrates every example.

- `lp-cli migrate --bump <kind>`:
  - The schema-bump workflow primitive. Mechanics:
    1. Snapshot current `examples/v<old>/<kind>/<name>.<kind>.toml`
       files as the immutable historical record (they stay where
       they are; future migrations operate against them).
    2. Copy `examples/v<old>/<kind>/*.toml` → `examples/v<new>/<kind>/*.toml`
       as the starting point for the new version.
    3. Snapshot current `schemas/v<old>/<kind>.schema.json` (it
       stays put; M4's gate 2 enforces immutability).
    4. Print clear next steps:
       - "Bump `CURRENT_VERSION` on the kind's struct."
       - "Write the migration in `src/migration/<kind>/v<old>_to_v<new>.rs`."
       - "Run `lp-cli migrate --all` to materialize the new examples."
       - "Run `lp-cli schema generate` to write the new schema."

- **Synthetic `v0_5 → v1` smoke test** (Q5):
  - Build a deliberately-fake `v0_5` corpus under
    `lp-domain/lp-domain/tests/migration_smoke/v0_5/<kind>/...`
    (NOT in `examples/`; this isn't real history, it's a test
    artifact).
  - Each fake `v0_5` file has `schema_version = 0_5` (or whatever
    encoding we pick for "less than 1") and exercises one
    migration primitive:
    - **Field rename**: a field that's named differently in v1.
    - **Struct reshape**: a section that moved or split.
    - **Default change**: a field whose default value changed.
    - **Optional → required**: a field that was optional in v0_5
      and is required in v1.
    - **Array length change**: an array whose `length` differs.
  - Write the corresponding `v0_5 → v1` migrations under
    `src/migration/<kind>/v0_5_to_v1.rs`.
  - CI test loads each `tests/migration_smoke/v0_5/...` file
    through `load_migrated` and asserts it deserializes to the
    expected v1 typed struct.
  - This proves the framework end-to-end **without requiring a real
    schema bump**. Future v1 → v2 bumps follow the same pattern but
    on real production examples.

- **CI gate 4 (forward compat / "frozen old parser")**: build
  binaries from a recent tagged commit (or vendor a snapshot of the
  Rust types under `tests/frozen/`); load every current
  `examples/v<latest>/<kind>/*.toml` through the old parser; assert
  success. Catches additive-only changes that snuck into a
  non-bumped version.

- **CI gate 5 (backward compat / history replay)**: for every kind,
  for every `tests/migration_smoke/v<N>/<kind>/*.toml` and every
  real `examples/v<N>/<kind>/<name>.<kind>.toml` for `N < latest`,
  run `load_migrated` and assert success. Catches broken or
  missing migrations.

- **CI gate 6 (immutability)**: hash check on the synthetic
  `tests/migration_smoke/v<N>/` corpus and on real
  `examples/v<N>/` directories where `N < latest` — any
  modification to a frozen file = fail. Same enforcement model as
  M4's schema immutability gate.

**Out of scope:**

- Down-migration (newer → older). Trait surface stays
  single-direction for now; down-migration is an opt-in future
  feature.
- Schema diff that understands semantic changes beyond the basic
  rules from M4.
- Cross-artifact migrations (e.g., a Live show that references a
  Pattern that just bumped). Each migration is local to its file.
- A "real" v1 → v2 bump on production examples — the synthetic
  smoke test is sufficient for v0; the first real bump happens
  organically when a domain change demands one.

## Key decisions

- **Synthetic corpus, not a forced production bump** (Q5). The
  framework gets exercised end-to-end without polluting the real
  example corpus with a contrived v1 → v2 change. The synthetic
  corpus stays in CI permanently as the regression guard.
- **`schema_version` is per-artifact-kind**, not global. Each kind
  evolves at its own pace.
- **Migrations operate on `toml::Value`**, then a single typed
  deserialize at the end. Hybrid model.
- **Migrations are non-fallible in the happy path** but return
  `Result` for malformed input.
- **History is permanent.** Once a file lands in
  `examples/v<N>/<kind>/` (for `N < latest`) or in
  `tests/migration_smoke/v<N>/`, it never changes. The
  immutability gate enforces this in CI.
- **`--bump` is a workflow helper, not a one-shot.** It does the
  mechanical snapshotting; the developer still has to write the
  migration code and edit `CURRENT_VERSION`.
- **Versioned directories** (`examples/v<N>/`, `schemas/v<N>/`,
  `tests/migration_smoke/v<N>/`) are the convention everywhere.
  Symmetric, easy to grep, and the M4/M5 tooling treats them
  uniformly.

## Deliverables

- `lp-domain/lp-domain/src/migration/` with registry, trait, helper
  functions.
- `lp-cli migrate <path>` and `lp-cli migrate --bump <kind>`
  commands.
- Synthetic `tests/migration_smoke/v0_5/` corpus across all six
  kinds (or as many as needed to exercise every migration
  primitive) plus the corresponding `v0_5 → v1` migrations under
  `src/migration/<kind>/v0_5_to_v1.rs`.
- CI gates 4, 5, 6 wired up (workflow yaml or `justfile` targets).
- Updated README documenting:
  - The bump workflow (`--bump <kind>`).
  - The `examples/v<N>/`, `schemas/v<N>/`,
    `tests/migration_smoke/v<N>/` conventions.
  - The CI gates and what each catches.

## Dependencies

- M4 complete (need schema codegen + the `schemas/v<N>/` layout
  convention to lay down history).
- M3 complete (need artifact types and the v1 example corpus to
  migrate against).

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: Most complex milestone in the roadmap. Migration
registry + two CLI subcommands + a synthetic-corpus smoke test that
has to round-trip cleanly + three new CI gates. Phases are needed
to keep the smoke test as an integration phase after the registry
and CLI bits land.

Suggested phase shape (decided during `/plan`, sketched here):

- Phase A: migration registry + trait + `load_migrated` plumbing —
  solo
- Phase B: `lp-cli migrate <path>` (no `--bump` yet) — depends on A
- Phase C: `lp-cli migrate --bump <kind>` workflow command —
  depends on A, parallel with B
- Phase D: synthetic `v0_5 → v1` corpus design (which primitives,
  which kinds) — solo, parallel with A/B/C
- Phase E: write the synthetic corpus + the corresponding
  migrations + the smoke test — depends on B+D
- Phase F: CI gates 4, 5, 6 — depends on E
- Phase G: README + bump-workflow doc + final verify

Estimated size: ~300 LOC registry + ~400–600 LOC CLI + ~200 LOC
synthetic corpus + ~200 LOC migrations + workflow yaml +
bump-workflow walkthrough.

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?
