# lp-domain roadmap notes

## Scope

Build the new Lightplayer domain model as a fresh top-level crate (or
crate group) at `lp-domain/`. This is the canonical Rust representation
of everything we've designed in `docs/design/lightplayer/` and
`docs/design/lpfx/`.

The roadmap covers:

1. **Domain model.** `Node`, `Property`, `NodePath`, `PropPath`,
   `NodePropSpec`, `ArtifactSpec`, `ChannelName`, `BindSource`. Traits +
   data structures, no I/O.
2. **Serialization.** TOML in/out for the artifact kinds we have
   examples for. Reuses `LpsValueF32` from `lps-shared` where possible.
3. **Schema versioning + migration.** Per-artifact-kind version field;
   migrations are `toml::Value → toml::Value`; typed deserialization
   only ever sees the latest format. Migration registry from day 1, even
   when there's nothing to migrate yet.
4. **Initial examples.** Move/copy the eight files in
   `docs/design/lpfx/` (six visuals + one stack + one playlist + one
   live) into `lp-domain/examples/` as the canonical test corpus.
   Loader/round-trip tests run against this corpus.
5. **Migration CLI.** `lp-cli` subcommand that walks a directory and
   rewrites artifacts to the latest version. `--check` mode for CI.
6. **lpfx integration (if feasible).** Wire the existing `lpfx` crate
   to read Visual artifacts via `lp-domain` instead of its current
   `FxManifest`. Render path stays as-is initially.

## Current state of the codebase

- **`docs/design/lpfx/`** has 8 example TOMLs in canonical "designed"
  form: rainbow, fbm, fluid (patterns); tint, kaleidoscope (effects);
  crossfade, wipe (transitions); psychedelic (stack); main (live);
  setlist (playlist).
- **`lpfx/lpfx/`** is a `no_std + alloc` crate with an older artifact
  format (`FxManifest`, `FxInputDef`, `FxValue`). Parses `fx.toml` with
  `[meta]` + `[input.K]` sections. Has tests against
  `examples/noise.fx/fx.toml`. Does NOT match the new design.
- **`lp-core/lp-model/`** is the legacy wire/transport model for the
  current engine. Has `serde`, `base64`, `serde-json-core`, `hashbrown`.
  Conflated networking + data model. Treat as legacy zone; do not
  modify.
- **`lp-shader/lps-shared/`** has foundational value/path/type code:
  `LpsValueF32`, `LpsValueQ32`, `value_path.rs` (PropPath equivalent),
  `path.rs`, `types.rs`. Reusable for lp-domain.
- **`lp-core/lp-shared/src/fs/`** has the `LpFs` filesystem abstraction
  (trait + `LpFsStd`, `LpFsMemory`, `LpFsView`, `FsChange`, `FsVersion`).
  Foundational for ArtifactSpec resolution and hot reload. Currently in
  the wrong place — needs to be pulled out into `lp-base/lpfs/` so
  `lp-domain` can depend on it without depending on legacy `lp-core`.
- **`lp-cli/src/commands/`** has subcommand structure (`profile/`,
  `serve/`, `create/`, etc.). Easy to add `migrate/`.
- No top-level `lp-domain/` crate exists yet.

## Notes

### Artifact resolution is its own architectural topic

Surfaced during Q11. Artifact loading and resolution is a fundamental
architectural component that deserves its own roadmap. **In scope for
this roadmap** is only the minimum needed to load the canonical
examples and run round-trip tests:

- `ArtifactSpec` as a typed string with parsing
- File-relative resolution (the only mode the examples use today)
- `LpFs::read` to fetch bytes
- Pass through migration + typed deserialize

**Out of scope, deferred to a future roadmap:**

- Library/registry concepts (`lib:/`, `std:/` prefixes)
- Hot reload (FsChange tracking, dependency invalidation)
- Project-relative vs file-relative resolution rules
- Caching, deduplication, asset content hashing
- Cross-artifact validation (Stack referencing a missing pattern, etc.)
- Cycle detection on Visual references
- Asset bundling for deployment

The v0 loader uses a deliberately simple resolution model. The future
roadmap (something like "lp-domain artifact resolution") replaces it
without touching the schema/migration core.

## Open questions

(Each gets asked one at a time via chat.)

### Q1. Single `lp-domain` crate or split `lpd-*` crates from day 1?

**Decided:** single crate, laid out as `lp-domain/lp-domain/` matching
the existing `lpfx/lpfx/` and `lp-base/lp-perf/` pattern. The outer
`lp-domain/` directory is the natural home for `lpd-*` sub-crates when
we split; the inner `lp-domain` crate becomes the umbrella re-export
at that point. Internal modules: `node`, `signal`, `binding`,
`artifact`, `schema`, `types`.

### Q2. Schema version key name in TOML?

**Decided:** `schema_version` (top-level `i32`, e.g. `schema_version = 1`).
Reserves `version` for possible user-facing artifact-version semantics.

### Q3. Per-artifact-kind versioning + schema generation + examples-by-version

**Decided:** per-kind versioning, JSON Schemas codegenned and checked in
from day one, canonical examples versioned alongside.

Concrete shape:

- **Kind** is implicit from filename suffix (`*.pattern.toml`,
  `*.live.toml`); migration registry keys on `(kind, from_version)`.
- **JSON Schemas** are generated via `schemars` from the Rust types and
  committed under:

  ```
  lp-domain/lp-domain/schemas/
    pattern/
      pattern-schema.json              # latest; regenerated on bump
      history/
        pattern-schema-v1.json         # immutable once merged
        pattern-schema-v2.json         # immutable once merged
    effect/
      effect-schema.json
      history/
        effect-schema-v1.json
    ...
  ```

  Layout rationale:

  - **Default = latest**: `pattern-schema.json` (no version suffix) is
    the current schema. Tools that "just want the schema" pick this
    file with no version logic.
  - **Filenames stand on their own**: `pattern-schema.json` is
    self-describing in greps and copy-pastes. `pattern.json` would
    look like a pattern artifact.
  - **Granularity is per-type**: `schemas/<kind>/history/` not
    `schemas/history/<kind>/`. We version each artifact kind
    independently (Q3 per-kind decision); the directory layout
    mirrors that.

- **CLI command** generates schemas: `lp-cli schema generate` (writes
  `<kind>-schema.json` for each kind). CI runs it, then `git diff
  --exit-code` to enforce that schemas were regenerated.
- **Immutability check**: separate CI step computes hashes of all
  files in `schemas/*/history/`. If any changed, fail loudly. Only
  `<kind>-schema.json` (the current/latest) is allowed to change in
  a single PR.
- **Bumping a version** is a deliberate act:
  1. Author writes a migration `migrate_pattern_v1_to_v2`.
  2. Bumps `Pattern::CURRENT_VERSION` from 1 to 2.
  3. Runs `lp-cli schema generate` → snapshots the previous
     `pattern-schema.json` into `history/pattern-schema-v1.json`,
     then writes the new `pattern-schema.json`.
  4. Runs `lp-cli migrate` → migrates the canonical examples and
     snapshots the old versions to `examples/<kind>/history/`.

- **Canonical examples** live at:

  ```
  lp-domain/lp-domain/examples/
    pattern/
      rainbow.pattern.toml          # latest, hand-edited
      fbm.pattern.toml
      fluid.pattern.toml
      history/                      # auto-generated, immutable
        rainbow-v1.pattern.toml
        fbm-v1.pattern.toml
        fluid-v1.pattern.toml
    effect/
      tint.effect.toml
      kaleidoscope.effect.toml
      history/
        tint-v1.effect.toml
        kaleidoscope-v1.effect.toml
    ...
  ```

  - Latest examples live at the kind directory root. Historical
    versions live in `<kind>/history/` with a `-vN` suffix in the
    basename (before the `.<kind>.toml` chain). This avoids dozens of
    identically-named `rainbow.pattern.toml` files across version
    subdirs — fuzzy-find in the IDE returns exactly one
    `rainbow.pattern.toml` (the latest); historical versions have
    unique names.
  - Stable paths for the latest mean
    `git log examples/pattern/rainbow.pattern.toml` shows the real
    evolution through migrations.
  - The `.<kind>.toml` suffix is preserved on history files so the
    loader can dispatch by kind on either path.
  - On version bump, `lp-cli migrate` copies each
    `examples/<kind>/<name>.<kind>.toml` to
    `examples/<kind>/history/<name>-v<old>.<kind>.toml`, then rewrites
    the latest in place to the new version. Both checked in;
    `history/` is enforced immutable by CI hash check.

- **CI gates**:
  1. `lp-cli schema generate` then `git diff --exit-code` (schemas
     regenerated and committed).
  2. Hash check on `schemas/*/v[N].json` (N < current) and `examples/history/`
     (frozen artifacts unchanged).
  3. Round-trip: load every file in `examples/latest/`, write it back,
     compare textually.

This is "examples-driven schema". Adding a new field requires bumping
the version, writing a migration, and seeing every example file's diff
in the PR. Painful in the right way.

### Q4. Migration mechanism + compatibility model

**Decided:** hybrid migrations (option c) + single `schema_version`
field bumped only on breaking changes. Forward compat by discipline,
backward compat by migrations, down-migration as future opt-in.

#### Migration mechanism

- Migrations operate on `toml::Value`: `fn migrate(value: &mut Value)`.
- Registry chains them until `schema_version == CURRENT_VERSION`, then
  a single typed `Deserialize` lands the value in current Rust types.
- Old typed structs (`PatternV1`, etc.) never exist. Only old TOML
  examples and migration functions.

```rust
pub trait Artifact: DeserializeOwned + JsonSchema {
    const KIND: &'static str;
    const CURRENT_VERSION: u32;
}

pub trait Migration {
    const KIND: &'static str;
    const FROM: u32;
    fn migrate(value: &mut toml::Value);
}
```

#### Compatibility model

- Single `schema_version: u32` per artifact. Bumped **only on breaking
  changes**.
- Optional `produced_by: String` for diagnostics ("lp-cli 0.4.2").
  Metadata only, not a compat axis.
- **No second version field** (no `app_version`); avoid the confusion
  until we actually need it.

Change classification (enforced by tests, not by hope):

| Change | Bump? |
|---|---|
| Add optional field with default | No |
| Add new artifact kind | No (for existing kinds) |
| Add new enum variant | Pin with tests |
| Rename / remove / retype field | Yes |
| Change default value | Yes |
| Change required-vs-optional | Yes |

Serde discipline: `#[serde(default)]` for optional fields, **no
`deny_unknown_fields`**. Old parsers ignore new fields silently.

#### Forward compat (old reader, new file)

Free, by discipline. New fields have defaults; parsers ignore unknown.
Firmware on schema v2 keeps working when web app emits a v2 file with
new optional fields.

#### Backward compat (new reader, old file)

Migrations. New parser reads v1, runs `migrate_v1_to_v2`, deserializes.

#### Down-migration

Future opt-in. Once the app is stable, a new lp-app can support down
migration to make older lp-fw still work. Surface: `lp-cli migrate
--to <version>`. Not in v0.

#### CI gates (in addition to Q3 gates)

"By discipline" = enforced by automated test, not by hope or code
review. Three mechanisms, each catching a different failure mode:

**1. Old-parser-against-new-files** (forward-compat, semantic)

  CI builds lp-domain at the merge-base (or last released tag) and
  runs its loader against the branch's `examples/latest/` files. If
  it fails AND the branch didn't bump `schema_version`, CI fails with
  "breaking change without version bump."

  - Needs a small `lp-domain-load-all` binary (or `cargo test` with an
    env-var path).
  - Reference point: `git merge-base main HEAD` for PRs; switch to
    last-released tag once we ship.
  - Cost: builds lp-domain twice in CI.

**2. Schema-diff** (forward-compat, structural)

  Compare current `schemas/<kind>/v<N>.json` against the immutable
  `schemas/<kind>/v<N-1>.json`. A small custom checker (or a vendored
  json-schema-diff crate) verifies the diff is additive-only (new
  optional fields, new enum variants). Non-additive without a
  declared version bump → fail.

  - Faster than #1 (no second build); catches what the schema can
    express.
  - Doesn't catch semantic-only drift.

**3. History-replay** (backward-compat)

  Every file in `examples/history/v<N>/` deserializes against current
  Rust types via the migration chain. Round-trips on
  `examples/latest/`.

#### Acknowledged limitation

Pure semantic drift (a field still parses but its meaning changed)
isn't enforceable by tests; relies on human review. Tests catch the
broad class of breakages that *do* surface as parse failures or
structural diffs.

#### Near-term reality

We have no users yet. The immediate value of all this discipline is
**keeping examples in sync** when we change the schema. The broader
contract (forward compat for fw etc.) is for the future, but the
discipline lives day one — that's the only way it actually works.

### Q5. Reuse `LpsValueF32` from `lps-shared` or define a new
`LpdValue`?

**Decided:** reuse + extend. lp-domain depends on `lps-shared`. One
value type across shaders, bus, params, bindings means no conversion
at the most-trafficked boundary in the system. Anything missing
(named vector types, `bool`, `color` tag) is added to `lps-shared`.

#### Two layers: data types vs signal types

`LpsValueF32` is the **data type** layer — raw scalars, vectors,
structs. The **signal type** layer is the new domain-level semantic
type system that lp-domain adds on top. These are the canonical
domain types of Lightplayer:

- `Texture` (a 2D pixel buffer; backed by GPU/CPU texture)
- `Audio` (1D sample array)
- `AudioFFT` (1D frequency-bin array)
- `AudioLevel` (scalar / small array of RMS levels)
- `Beat` (TBD: BPM clock + downbeat flag)
- `Touch` (touch-points struct; XY + pressure + id + continuity)
- `Motion` (TBD: IMU data)
- `Float`, `Int`, `Bool` (basic scalars for params/modulators)
- `Vec2`, `Vec3`, `Vec4` (basic vectors)
- `Color` (RGB or RGBA, semantic-tagged)

Each `SignalType` maps to / is backed by an `LpsValueF32` shape.
Bus channels declare a `SignalType`. Params declare what
`SignalType` they accept. Bindings type-check at compose time:
"does this Source's signal type match this Param's expected type?"

**This is the core semantic type system of Lightplayer** — the
language of "what kind of thing is flowing through this wire." It
goes in `lp-domain/src/signal/types.rs` and is a major deliverable of
the crate.

(See `docs/design/lightplayer/domain.md` "Signal types" section for
the design sketch; the full type set is open and will be refined as
we build out signal sources/sinks.)

### Q6. Where do canonical examples live?

**Decided** (folded into Q3):
`lp-domain/lp-domain/examples/<kind>/<name>.<kind>.toml` is the
hand-edited latest. `lp-domain/lp-domain/examples/<kind>/history/<name>-v<N>.<kind>.toml`
is the auto-generated, immutable archive. `docs/design/lpfx/` stops
shipping .toml files and links to canonical paths instead.

### Q7. CLI command shape?

Q3 added a second command (schema generation). Now the surface is
roughly:

- `lp-cli schema generate [--out PATH]` — regenerate JSON Schemas
  from Rust types. Default output is
  `lp-domain/lp-domain/schemas/`. Used in CI to verify schemas are
  in sync with code.
- `lp-cli migrate <path>` — walk a directory, run migrations on
  every artifact found. `--check` (CI dry-run, exits non-zero if
  anything would change), `--diff` (show diffs without writing).
- `lp-cli migrate --bump <kind>` — internal helper used when
  authoring a version bump: snapshots `examples/latest/<kind>/`
  into `examples/history/<kind>/v<old>/`, then migrates `latest/`
  in place.

**Decided:** top-level `lp-cli migrate` and `lp-cli schema`
subcommands. Logic lives in `lp-domain`; CLI is a thin wrapper that
imports `lp_domain::schema` and `lp_domain::migrate`.

### Q8. lpfx bridging strategy?

**Decided:** sequential, no preservation needed. lpfx is brand new and
not load-bearing — nothing in production depends on `FxManifest` /
`FxModule`. Plan:

1. Build the core lp-domain work (types, schemas, migration, examples,
   CLI). Stand-alone, fully tested.
2. Update lpfx: either rewrite from scratch on top of lp-domain, or
   migrate the existing types to lp-domain. Tactical choice made when
   we get to that milestone — depends on how much of the current code
   is worth keeping at that point. Either way: no parallel-coexistence
   period, no compat shim.

This means the lp-domain core can ship and be validated against
canonical examples before any lpfx work begins. Removes the
"if feasible" caveat from the original scope: lpfx integration moves
from a stretch goal to a planned (but later) milestone.

### Q9. `no_std` target for lp-domain?

**Decided:** `lp-domain` is `no_std + alloc` by default. Primary
target is **ESP32-C6** (P4 is future). Anything that runs on-device
must be `no_std`: domain model, parsers, schema-version-checking,
binding resolution, Visual artifact loaders.

General rule across the workspace: target `no_std` if at all feasible.

#### Confirmed feasibility

- `toml` 0.9 with `default-features = false, features = ["parse",
  "serde"]` compiles in `no_std + alloc`. TOML parsing works
  on-device.
- `serde` derive works in no_std with `alloc` feature.
- `lps-shared` is already `no_std + alloc`.
- `LpFs` (after lpfs extraction) is no_std-friendly with std behind a
  feature flag.

#### Feature gating

```toml
# lp-domain/lp-domain/Cargo.toml
[features]
default = []                # no_std + alloc by default
std = ["lpfs/std", "toml/std"]   # file IO via LpFs, etc.
schema-gen = ["std", "dep:schemars"]   # host-side codegen only
```

Only host-side tools enable `std` and `schema-gen`:

- `lp-cli` enables `std` + `schema-gen` (runs migrator + schema gen)
- desktop server / engine enable `std`
- `lp-fw` and on-device builds use default (`no_std + alloc`)

#### Module layout

```
lp-domain/lp-domain/src/
  types.rs          # no_std
  node/             # no_std (Node trait, Property model, runtime tree)
  signal/           # no_std (SignalType, ChannelName)
  binding.rs        # no_std (BindSource, resolution)
  schema/
    mod.rs          # no_std (CURRENT_VERSION constants, Artifact trait)
    registry.rs     # no_std (migration registry, runs on-device for
                    #         schema-version-aware loads)
    gen.rs          # schema-gen only (schemars codegen)
  artifact/
    mod.rs          # no_std (typed artifact structs)
    parse.rs        # no_std (TOML → typed via migration chain)
    load.rs         # std only (LpFs-based loader; on-device may also
                    #           load via embedded LpFsView)
  migrate/          # no_std (migration fns themselves)
```

Migrations themselves are no_std — they run on-device when an old
artifact is loaded. The CLI tool that *runs* the migrator
batch-style is std.

### Q11. lpfs extraction — scope?

**Decided:** move just `fs/` → `lp-base/lpfs/` as Milestone 1.
Other foundational bits in `lp-shared` stay where they are unless a
pattern emerges. Mechanical move with all call sites updated in the
same PR; no transitional re-export.

Crate shape:

```
lp-base/lpfs/
  Cargo.toml          # no_std + alloc, std feature for LpFsStd
  src/
    lib.rs
    lp_fs.rs          # trait
    lp_fs_mem.rs
    lp_fs_std.rs      # std-only
    lp_fs_view.rs
    fs_event.rs       # FsChange, FsVersion
```

Call sites to update: `lp-core/lp-engine`, `lp-core/lp-server`,
`lp-cli/src/commands/dev/`, `lp-fw/`, etc.

### Q10. Initial Visual kind coverage?

**Decided:** v0 implements the **loader / typed-struct layer for all
six Visual kinds**. Runtime behavior (Node implementations, render
hookup) is out of scope for this roadmap and lands in the
lpfx-rewrite roadmap that follows.

Why all six: every example file round-trips, every JSON Schema gets
exercised on day one, no kind silently lacks a loader. The schema
machinery has six real customers, not three.

#### Refinement: collapse ShaderX vs BuiltinX

Drop the `ShaderPattern` / `BuiltinPattern` (and parallel
`ShaderEffect` / `ShaderTransition`) split. There is just `Pattern`,
`Effect`, `Transition`. Each has a `[shader]` section whose contents
can be:

- inline source (GLSL today, possibly WGSL later)
- a reference to a builtin shader name (the engine ships a registered
  shader with multiple backends: GLSL for lpvm, WGSL for wgpu,
  possibly a CPU impl for cases like fluid that don't realistically
  fit in a fragment shader)

The multi-backend story lives in the **shader layer**, not at the
Visual artifact layer. From the artifact's perspective, "fluid" is
just a Pattern whose shader happens to be a builtin name. From the
runtime's perspective, the shader resolver picks the right backend
for the current target.

This means the example files change shape during v0:

- Today: `fluid.pattern.toml` uses `[builtin] impl = "fluid"` plus
  config-as-params.
- Future v0: `fluid.pattern.toml` uses `[shader] builtin = "fluid"`
  plus regular `[params]`. Same file, simpler model.

Concrete typed structs to deliver in v0 (one per kind, no subkinds):

| Kind | Typed struct | Inline + builtin shader |
|---|---|---|
| Pattern | `pattern::Pattern` | yes |
| Effect | `effect::Effect` | yes |
| Transition | `transition::Transition` | yes |
| Stack | `stack::Stack` | n/a (composes other Visuals) |
| Live | `live::Live` | n/a (selects other Visuals) |
| Playlist | `playlist::Playlist` | n/a (sequences other Visuals) |

(Show, Rig, Project are different artifact tiers and not in this
roadmap.)
