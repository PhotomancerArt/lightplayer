# lp-domain roadmap

## Motivation / rationale

Lightplayer's domain model has crystallized over weeks of design — Node,
Property, Bus, Signal, Module, Visual, Binding, ArtifactSpec, plus the
Quantity model (Kind, Shape, Slot, Constraint, ValueSpec,
Presentation) — and is captured in three authoritative design docs:

- [`docs/design/lightplayer/domain.md`](../../design/lightplayer/domain.md)
  — broad Lightplayer vocabulary (Visuals, Modules, Rigs, Bus, Project).
- [`docs/design/lightplayer/quantity.md`](../../design/lightplayer/quantity.md)
  — the Quantity model that lp-domain implements.
- [`docs/design/color.md`](../../design/color.md) — color strategy
  and numeric precision contract.

Plus the lpfx design under `docs/design/lpfx/` and accompanying
notes. The current `lp-core/lp-model` is a wire/transport model
conflated with a data model and doesn't reflect any of this. Trying
to refactor in place is a stop-the-world exercise that would break
the existing engine while in flight.

This roadmap stands up a fresh `lp-domain` crate that *is* the
canonical Rust representation of what we designed, with schema
versioning + migration discipline baked in from day one. The discipline
matters now (so it survives later when we have users), even though
near-term the only practical benefit is keeping examples in sync as
the schema evolves.

Concrete near-term goals:

1. New `lp-domain/lp-domain/` crate, no_std + alloc by default
   (primary target ESP32-C6).
2. Typed structs for all six Visual kinds (Pattern, Effect, Transition,
   Stack, Live, Playlist).
3. Schema versioning via `schema_version` field; per-kind versioning;
   `toml::Value`-based migration registry (typed deserialize at end).
4. JSON Schemas codegenned (schemars) and checked in. Immutability of
   frozen versions enforced by CI.
5. Canonical examples in `lp-domain/lp-domain/examples/`, with
   immutable historical snapshots in `<kind>/history/`.
6. `lp-cli schema generate` and `lp-cli migrate` commands.
7. CI gates that turn the discipline into mechanical enforcement.

lpfx (the existing crate) is **not** rewired in this roadmap — that's a
follow-up. lpfx is brand new and not load-bearing; its update happens
sequentially after the lp-domain core is solid.

## Architecture / design

### Crate layout

```
lp-base/lpfs/                   # NEW (Milestone 1) — extracted from lp-core/lp-shared/src/fs/
  src/
    lib.rs
    lp_fs.rs                    # trait
    lp_fs_{mem,std,view}.rs
    fs_event.rs

lp-domain/lp-domain/            # NEW (Milestone 2+) — the new domain model
  Cargo.toml                    # no_std + alloc default; std + schema-gen features
  examples/                     # versioned: examples/v<N>/<kind>/...
    v1/                         # the v1 baseline corpus (M3)
      pattern/
        rainbow.pattern.toml
        fbm.pattern.toml
        fluid.pattern.toml
      effect/
        tint.effect.toml
        kaleidoscope.effect.toml
      transition/
      stack/
      live/
      playlist/
    # v2/, v3/, ... appear when M5's --bump workflow runs
  schemas/                      # versioned: schemas/v<N>/<kind>.schema.json
    v1/
      pattern.schema.json
      effect.schema.json
      transition.schema.json
      stack.schema.json
      live.schema.json
      playlist.schema.json
    # v2/, ... appear on bumps
  tests/
    migration_smoke/            # synthetic v0_5 corpus (M5 — Q5)
      v0_5/
        pattern/...
        ...
  src/
    lib.rs                      # re-exports lps-shared::LpsType, LpsValueF32 as LpsValue
    types.rs                    # UID, Name, ChannelName, NodePath, PropPath, NodePropSpec, ArtifactSpec (no_std)
    kind.rs                     # Kind enum + per-Kind impls; Dimension, Unit, Colorspace, InterpMethod (no_std)
    constraint.rs               # Constraint enum (no_std)
    shape.rs                    # Shape, Slot (no_std)
    value_spec.rs               # ValueSpec, TextureSpec, materialize() (no_std)
    binding.rs                  # Binding enum, BindingResolver trait (no_std)
    presentation.rs             # Presentation enum (no_std)
    node/                       # Node trait, Property model (no_std)
    schema/
      mod.rs                    # Artifact trait, CURRENT_VERSION (no_std)
      gen.rs                    # schemars codegen (schema-gen feature, host-side)
    migration/                  # migration registry + per-kind fns (no_std)
    artifact/
      mod.rs                    # trait re-exports (no_std)
      parse.rs                  # TOML → typed via migration chain (no_std)
      load.rs                   # LpFs-based loader (std)
    visual/                     # Pattern, Effect, Transition, Stack, Live, Playlist
      pattern.rs
      effect.rs
      transition.rs
      stack.rs
      live.rs
      playlist.rs

lp-cli/src/commands/
  schema/                       # NEW — `lp-cli schema generate`
  migrate/                      # NEW — `lp-cli migrate`

lpfx/lpfx/                      # NOT TOUCHED in this roadmap; sequential follow-up
```

### Data flow on load

```
TOML file on disk
    │
    ▼  LpFs::read
raw bytes
    │
    ▼  toml::from_str
toml::Value                                ◄── schema_version = N
    │
    ▼  migrate registry chain (Value → Value)
toml::Value at CURRENT_VERSION
    │
    ▼  serde::Deserialize  (uses #[serde(default)] for additive compat)
typed Visual struct (Pattern / Effect / Stack / ...)
    │
    ▼  validation (schema check, binding resolution at compose time)
Loaded Node tree
```

### Compatibility model

- Single `schema_version: u32` per artifact, bumped only on breaking
  changes.
- Optional `produced_by: String` for diagnostics.
- Forward compat (old reader, new file) by serde-default discipline +
  CI tests. Most schema changes are additive and require no bump.
- Backward compat (new reader, old file) by migrations. Append-only;
  hybrid model — `toml::Value` migrations chain until current, then a
  single typed `Deserialize`. Old typed structs never exist in code.
- Down-migration is a future opt-in (lets a new lp-app talk to old
  lp-fw); not in v0.

### Discipline = automated tests

"By discipline" means "enforced by automated test, not by hope or code
review." Six CI mechanisms (M4 lands gates 1–3, M5 lands 4–6):

1. **Schema drift (M4)**: `lp-cli schema generate` then `git diff
   --exit-code`. Catches "I changed the model but forgot to
   regenerate."
2. **Schema immutability (M4)**: hash check on `schemas/v<N>/`
   directories where `N < latest`. Any modification = fail.
3. **Schema additive-vs-breaking (M4)**: schema-diff utility
   compares current schema against the previous frozen one.
   Non-additive without `CURRENT_VERSION` bump = fail.
4. **Forward compat / "old parser, new files" (M5)**: build lp-domain
   at the merge-base (or last released tag), run its loader against
   the branch's `examples/v<latest>/<kind>/*.toml`. Failure without
   a `schema_version` bump = breaking change without ceremony.
5. **Backward compat / history replay (M5)**: for every kind, for
   every file in `tests/migration_smoke/v<N>/<kind>/` and for every
   real `examples/v<N>/<kind>/<name>.<kind>.toml` where `N < latest`,
   `load_migrated` and assert success.
6. **Example immutability (M5)**: hash check on
   `examples/v<N>/<kind>/` and `tests/migration_smoke/v<N>/`
   directories where `N < latest`. Any modification = fail.

Acknowledged limit: pure semantic drift (a field still parses but
means something different) isn't catchable by tests; relies on review.

## Alternatives considered

- **Refactor `lp-core/lp-model` in place** — too tangled with transport
  concerns; would break the existing engine while in flight.
- **Six `lpd-*` crates from day 1** — forces premature seams while
  names/shapes are still moving. Single `lp-domain/lp-domain/` with
  internal modules; split later when isolation forces it.
- **No version field, ProtoBuf-style discipline only** — no carry for
  breaking changes when they happen. Single field bumped only on
  breaking is the pragmatic middle.
- **Two version fields (schema + app)** — invites confusion about what
  each means; deferred until a concrete need exists.
- **Versioned Rust types (`PatternV1`, `PatternV2`)** — codebase grows
  linearly with version count; old structs become dead weight.
  Hybrid migrations on `toml::Value` keep current types clean.
- **Avro-style schema fingerprint resolution / Kubernetes-style
  multi-storage** — massive overkill at this scale.
- **Define a new `LpdValue` instead of reusing `LpsValueF32`** —
  conversion friction at every shader/bus/param boundary; reuse +
  extend is the lighter path.
- **Bare-string bindings as the day-1 form** (vs `bind = { bus = "time" }`)
  — easier to add sugar later than to take it away. (Decided in
  earlier design pass; carried into v0.)
- **Bundle runtime/render in this roadmap** — would balloon scope.
  Runtime/render belongs in the lpfx-rewrite roadmap that follows.
- **`latest/` and `history/` as top-level subdirs of examples/** —
  causes IDE friction with N identically-named files. **Versioned
  directories** (`examples/v<N>/<kind>/...`,
  `schemas/v<N>/<kind>.schema.json`) are clearer (every directory's
  schema_version is in the path; `examples/v1/` greps to "everything
  v1") and the M5 `--bump` workflow is mechanical (copy the v<old>
  directory to v<new>).
- **Versioned Rust types (`PatternV1`, `PatternV2`)** — codebase
  grows linearly with version count; old structs become dead
  weight. (Restated; same conclusion.)
- **`pattern.json` vs `pattern.schema.json` vs `pattern-schema.json`** —
  context-dependent name. We land on `pattern.schema.json` —
  short, conventional in JSON Schema land, the `.schema.` infix
  unambiguous.
- **A separate `SignalType` layer for Audio/Video/Touch/...** —
  recreates concept overlap with `Kind`. Q9 collapses signals into
  Kind variants (`Audio`, `Beat`, `Touch`, `Motion` are Kinds; bus
  channels are Kind-typed by their first binding).
- **`KindedValue` / `KindMeta` to attach semantic metadata to
  `LpsValue`** — bloats the value layer and creates double-tagging
  at the GPU boundary. Color-family Kinds carry their colorspace
  inside the struct itself (a regular field, not side metadata);
  semantic identity comes from the `Slot` the value is paired with.
- **Per-slot `colorspace` metadata** — backwards. The colorspace
  is part of the *value* (user picked it in OKLCH; round-tripping
  must preserve it). Lives inside the color-family `LpsValue::Struct`
  as a `space` field. See `color.md`.
- **Variable-length arrays for ColorPalette / Gradient** — bad fit
  for embedded targets. Fixed-max storage (`MAX_PALETTE_LEN = 16`,
  `MAX_GRADIENT_STOPS = 16`) plus an explicit `count: i32` field;
  shaders iterate `0..count`.

## Risks

- **`schemars` no_std**: schemars *codegen* is std-only.
  Mitigation: codegen lives behind the `schema-gen` feature
  (host-side only). The `JsonSchema` derives themselves are
  always-on (verified compiling no_std in M2 — Q6).
- **`schemars` and recursive `Shape`/`Slot` types**: risk that
  schemars chokes on recursion. Mitigation: M2 verifies recursive
  round-trip on day one (Q6 — incremental validation, not a
  pre-M2 spike). Fallback chain: manual `JsonSchema` impl →
  hand-written schema → alternative generator → drop schema
  generation.
- **`toml` 0.9 no_std edge cases**: confirmed compiling for basic types,
  but exotic schemas may surface limits. Mitigation: spike if anything
  unusual comes up; keep schemas straightforward.
- **Migration discipline atrophies**: the "by test, not by hope" model
  relies on CI gates being maintained. Mitigation: bake gates into CI
  from Milestone 5 onward; document them prominently in the crate README.
- **lpfs extraction breaks downstream**: many crates depend on the
  current `lp-shared::fs::*`. Risk is correctness of import path
  updates. Mitigation: full workspace `cargo build && cargo test`
  before merge; mechanical move with all call sites updated in the
  same PR.
- **Pattern collapse (`ShaderPattern` + `BuiltinPattern` → `Pattern`)**
  changes example file shape from what's currently in
  `docs/design/lpfx/`. Mitigation: do this *as part of* Milestone 3
  when examples land in `lp-domain/lp-domain/examples/`; design files
  get re-pointed afterward.
- **Artifact resolution model is intentionally minimal in v0** —
  may feel constraining when the lpfx rewrite arrives. Mitigation:
  explicitly scoped out and documented as a follow-up roadmap.
- **Forward-compat CI gate (mechanism #1) requires building lp-domain
  twice in CI**: extra build time per PR. Mitigation: cache the
  merge-base build; acceptable cost for the safety it provides.

## Scope estimate

Six milestones, each shippable independently. Per-milestone execution
strategy follows the three-option model from `/roadmap`: **A** =
direct execution from the milestone file (no plan), **B** =
`/plan-small` (single plan file, then implement), **C** = full
`/plan` (notes + design + numbered phases, then `/implement`).

| #  | Milestone                                                | Strategy | Detail file                                                  |
|----|----------------------------------------------------------|----------|--------------------------------------------------------------|
| M1 | lpfs extraction                                          | **A**    | [m1-lpfs-extraction.md](./m1-lpfs-extraction.md)             |
| M2 | lp-domain skeleton + foundational types                  | **C**    | [m2-domain-skeleton.md](./m2-domain-skeleton.md)             |
| M3 | Visual artifact types + canonical examples               | **C**    | [m3-visual-artifact-types.md](./m3-visual-artifact-types.md) |
| M4 | Schema generation + immutability + lp-cli schema         | **C**    | [m4-schema-generation.md](./m4-schema-generation.md)         |
| M5 | Migration framework + lp-cli migrate + compat CI gates   | **C**    | [m5-migration-framework.md](./m5-migration-framework.md)     |
| M6 | Integration validation + cleanup                         | **B**    | [m6-integration-validation.md](./m6-integration-validation.md) |

Reasoning at a glance:

- **M1 (A — direct):** mechanical move of `lp-core/lp-shared/src/fs/`
  to a new crate plus a call-site sweep. No design questions; scope
  pinned by the milestone file. Composer 2 sub-agent executes
  end-to-end.
- **M2 (C — full plan):** foundational types — identity types
  (UID, NodePath, ArtifactSpec, ChannelName), the Quantity model
  (Kind, Constraint, Shape, Slot, ValueSpec, Binding,
  Presentation), and the trait surface (Node, Artifact, Migration).
  schemars derives go on day one (Q6). 5–6 phases, parallelisable
  after the foundational `types.rs` lands. Larger than the original
  estimate because the Quantity model resolved into substantial
  surface; see [`quantity.md`](../../design/lightplayer/quantity.md).
- **M3 (C — full plan):** six Visual kinds + shared substructures +
  TOML grammar for `Slot` (Q7 — custom `Deserialize`) + example
  move to versioned `examples/v1/` directory + design-doc cleanup +
  signal Kinds added incrementally as examples need them (Q9 —
  Audio/Beat/Touch/...). Leaf-vs-composed Visual phase split lets
  two sub-agents work in parallel after the shared substructures
  and the TOML grammar land.
- **M4 (C — full plan):** pure tooling now (Q6 made the schemars
  derive sweep land in M2 instead). Codegen module, `lp-cli schema
  generate`, custom schema-diff utility, three CI gates. Smaller
  than the original estimate.
- **M5 (C — full plan):** most complex milestone. Migration
  registry, two CLI subcommands, **synthetic `v0_5 → v1` smoke
  test** (Q5 — a deliberately-fake old corpus that exercises every
  migration primitive without forcing a contrived production bump),
  three more CI gates. Smoke-test design is its own phase.
- **M6 (B — small plan):** verification + README + cross-references.
  No architectural design, but the verification command list and
  surprise-tracking benefit from a single plan file.

lpfx rewrite is a separate roadmap; not part of this one.
