# Scope of Work

Milestone 3 migrates the legacy shader / texture / fixture / output source
shape from per-node `node.json` config files and legacy trait-object config
loading toward TOML-backed `lpc-source` shapes.

The plan can be developed while M2.1 is being implemented, but M3 should treat
the M2.1 runtime-product API as a dependency boundary. M3 planning should avoid
hard-coding exact M2.1 symbol names until that milestone lands, except where the
M2.1 design has already settled the concept: resolver outputs may be direct
values or render-product handles.

In scope:

- Define TOML-backed legacy MVP source artifacts/configs for shader, texture,
  fixture, and output.
- Reuse `lpc-source::legacy` as the home for legacy source shapes rather than
  reintroducing separate `lpl-*` crates.
- Map existing legacy config fields to `SrcNodeConfig`, `SrcArtifactSpec`,
  `SrcBinding`, `SrcValueSpec`, and existing model/source value vocabulary.
- Add loading tests for TOML source artifacts and legacy node config TOML.
- Add a deliberate compatibility story for existing `node.json` fixtures,
  examples, builders, and tests.
- Keep `LegacyProjectRuntime` available while source migration lands.

Out of scope:

- Porting concrete shader / fixture / output runtime behavior to the new core
  engine.
- Retiring `LegacyProjectRuntime`.
- Retiring JSON loading everywhere in one step.
- Implementing real shader-backed render products or texture-product storage.
- Reworking the M2.1 runtime-domain implementation while it is in flight.

# Current State

`lpc-source` already owns legacy source modules:

- `lp-core/lpc-source/src/legacy/nodes/shader/config.rs`
- `lp-core/lpc-source/src/legacy/nodes/texture/config.rs`
- `lp-core/lpc-source/src/legacy/nodes/fixture/config.rs`
- `lp-core/lpc-source/src/legacy/nodes/output/config.rs`
- `lp-core/lpc-source/src/legacy/nodes/kind.rs`

Those legacy config structs are still the old node config shape. They implement
`NodeConfig`, serialize/deserialize through serde, and are loaded as
`Box<dyn NodeConfig>` by legacy runtime code.

The active legacy loader is still directory and JSON centered:

- `legacy_load_node` derives node kind from directory suffixes such as
  `.shader`, `.texture`, `.fixture`, and `.output`.
- It reads `<node-dir>/node.json`.
- It deserializes directly into `ShaderConfig`, `TextureConfig`,
  `FixtureConfig`, or `OutputConfig`.
- `LegacyProjectRuntime::load_nodes`, hot reload, delete handling, and the test
  project builder all assume `node.json` as the node config sentinel.

`SrcNodeConfig` already exists as the new per-instance source shape:

- It stores an authored `artifact: SrcArtifactSpec`.
- It carries per-instance overrides as `(PropPath, SrcBinding)`.
- It already has JSON and TOML round-trip tests.

`lpc-source` also already has TOML artifact infrastructure:

- `SrcArtifact` defines `KIND`, `CURRENT_VERSION`, `schema_version`, and
  `walk_slots`.
- `load_artifact` reads a TOML file, deserializes a typed artifact, validates
  schema version, and materializes embedded defaults.
- `SrcBinding` supports bus, literal, and node-prop references.
- `SrcValueSpec` supports authored literals plus texture recipes such as
  `SrcTextureSpec::Black`.

`lpfs` already depends on `lpc-source`:

- `lp-base/lpfs/Cargo.toml` has `lpc-source = { path = "../../lp-core/lpc-source", default-features = false }`.
- `lp-base/lpfs/src/lpc_model_artifact.rs` implements
  `lpc_source::ArtifactReadRoot` for `LpFsMemory`, `LpFsView`, `dyn LpFs`, and
  `LpFsStd`.
- A direct `lpc-source -> lpfs` dependency would create a crate cycle.
- The existing pattern is therefore: source-owned generic traits and typed
  parsing in `lpc-source`; filesystem-specific trait implementations in `lpfs`.

The live `node.json` migration footprint is concentrated:

- `lpc-engine/src/legacy_project/legacy_loader.rs` discovers node directories
  and loads `node.json`.
- `lpc-engine/src/legacy_project/project_runtime/core.rs` uses `node.json`
  deletion as a node-removal sentinel and routes creates/modifies to the legacy
  loader.
- `lpc-engine/src/legacy/project.rs` reloads `node.json` during init and config
  hot reload because runtime config is stored behind `Box<dyn NodeConfig>`.
- `lpc-shared/src/project/builder.rs` writes all test/project-builder node
  configs as `node.json`.
- `lpc-engine/tests/scene_update.rs` and
  `lpc-engine/tests/partial_state_updates.rs` directly modify/delete
  `node.json`.

Additional live templates/tests outside `lpc-engine` also encode `node.json`:

- `lp-cli/src/commands/create/project.rs` creates new projects with `node.json`
  files using the current legacy config structs.
- `lp-app/lpa-server/src/template.rs` also writes `node.json`, but its inline
  JSON appears to be an older config shape and should be audited during the
  wholesale switch.
- `lp-app/lpa-server/tests/server_tick.rs` and
  `lp-app/lpa-server/tests/stop_all_projects.rs` copy `node.json` files from
  `ProjectBuilder` into a server filesystem.
- `lp-fw/fw-esp32/src/tests/fluid_demo/ring_geometry.rs` documents that its
  generated geometry mirrors an example fixture `node.json`.

There are also real example project files, not only code references:

- `examples/basic`, `examples/basic2`, `examples/fast`,
  `examples/perf/baseline`, and `examples/perf/fastmath` contain 20
  `node.json` files total.
- Sampled files use the current serde shape for the legacy config structs:
  shader `{ glsl_path, texture_spec, render_order, glsl_opts? }`, texture
  `{ width, height }`, fixture `{ output_spec, texture_spec, mapping, ... }`,
  and output `{ GpioStrip = { ... } }` equivalent JSON.
- These examples should be migrated if the runtime loader switches wholesale to
  `node.toml`; otherwise examples become dead data.

M2.1 is expected to add the runtime product boundary:

- Direct resolved values should flow as a value product.
- Render-like outputs should flow as small handles into engine-managed product
  storage.
- M3 should not force texture compatibility deeper into `ModelValue` or
  `LpsValueF32`.

# Questions

## Q1: Can M3 planning proceed while M2.1 is being implemented?

Context: M3 depends on the M2.1 concept that source-loaded values can eventually
materialize into runtime products, but M3 is mainly authored data and loading.

Suggested answer: Yes. Plan M3 now, but phrase phases around source-level
interfaces and the conceptual M2.1 output boundary. Avoid implementation phases
that require final M2.1 symbol names until after M2.1 lands.

Answer: Yes. Planning can proceed in parallel with M2.1.

## Q2: Should migrated legacy source types stay in `lpc-source::legacy`?

Context: M1 already moved legacy config/source concepts into the `lpc-*` family,
and `lpc-source::legacy::nodes` already contains shader, texture, fixture, and
output config modules.

Suggested answer: Yes. Keep the migrated legacy TOML/source structs under
`lpc-source::legacy`. Do not create new `lpl-source` or sibling domain crates.

Answer: Yes.

## Q3: Should M3 create core-named artifacts or explicitly legacy-named artifacts?

Context: The milestone asks whether migrated types should live as legacy modules
or directly named core source artifacts. The existing structs are legacy-specific:
`ShaderConfig` references GLSL path plus a texture target, `FixtureConfig`
references texture/output specs, and the directory suffixes are legacy node
kinds.

Suggested answer: Use explicitly legacy-named source artifacts/configs for this
MVP slice. The artifact loader can still produce `SrcNodeConfig` and bindings,
but names should not imply these are the final core visual/fixture abstractions.

Answer: Yes.

## Q4: What should be the new TOML file/sentinel shape for node configs?

Context: Existing project loading discovers node directories by suffix and uses
`node.json` as both the config file and deletion sentinel. `SrcNodeConfig`
already TOML-serializes as a single file with an artifact reference and
overrides.

Suggested answer: Introduce a TOML node config sentinel, likely `node.toml`, for
`SrcNodeConfig`-style authored node instances. Keep `node.json` as a legacy
compatibility path during M3.

Answer: Yes, likely `node.toml`. Single-file nodes probably are not supported in
this migration; keep node directories as the authored unit.

## Q5: Should legacy `node.json` support remain in M3?

Context: Tests, examples, hot reload, and `lpc-shared::project::builder` still
write and watch `node.json`. Removing it all at once would turn the source
migration into a runtime/test migration.

Suggested answer: Originally: keep JSON compatibility as a clearly named
compatibility loader and update only selected tests/builders to exercise TOML.

Answer: Reopen. Keeping JSON raises the question of when it gets removed.
Switching wholesale during M3 adds scope but may be better because it leaves the
milestone actually finished. Need a scoped migration estimate before deciding.

## Q6: Should texture compatibility be included in M3?

Context: The milestone says texture compatibility only if required by the
shader -> fixture path. Current legacy shader config targets a texture, and
fixture config samples a texture. Without texture config compatibility, the MVP
legacy source graph cannot represent the existing path.

Suggested answer: Yes, include texture config as a compatibility artifact/source
shape in M3. Treat it as a bridge for the existing shader -> texture -> fixture
flow, not as the final render-product abstraction.

Answer: Yes.

## Q7: How should legacy `NodeSpec` references map into `SrcNodeConfig` and bindings?

Context: Legacy configs directly reference other node directories via
`NodeSpec`: shader -> texture, fixture -> texture, fixture -> output. New source
shapes prefer `SrcBinding::{Bus, NodeProp, Literal}` and artifact references.

Suggested answer: Preserve direct `NodeSpec` references inside the legacy
compatibility config for M3, and add conversion helpers that can produce
`SrcNodeConfig`/binding equivalents where the target core engine path needs
them. Do not force every legacy edge through a bus channel yet.

Answer: Likely yes. We probably need to preserve/directly support these
references for the legacy MVP migration.

## Q8: Where should TOML artifact loading integration live?

Context: `lpc-source` has generic typed artifact loading without an `lpfs`
dependency, while legacy project loading lives in `lpc-engine` and uses `LpFs`.

Suggested answer: Originally: keep typed source structs and serde/TOML tests in
`lpc-source`; keep filesystem discovery, compatibility selection, and
`LegacyProjectRuntime` integration in `lpc-engine`.

Answer: Reopen. Loading is useful outside the engine and should be pushed as far
into `lpc-source` as the dependency graph allows. Because `lpfs` already depends
on `lpc-source`, `lpc-source` cannot directly depend on `lpfs`; use generic
source-owned read/discovery traits, with `lpfs` implementing them.

## Q9: How much should M3 update generated/test project builders?

Context: `lpc-shared::project::builder` writes only `node.json` today. Existing
runtime tests rely on that builder and are valuable coverage for the legacy
runtime.

Suggested answer: Originally: add TOML-capable builder helpers or fixtures for
new source loading tests, but leave default legacy runtime builders on JSON until
the runtime consumes TOML by default.

Answer: Reopen. A wholesale builder/runtime/test switch may be annoying but
preferable if the live footprint stays concentrated. Need to inspect whether
tests, app templates, CLI templates, and demos can all move in this milestone
without dragging in unrelated runtime changes.

Follow-up finding: The wholesale switch touches a real but bounded set of live
Rust files: the legacy loader/runtime reload path, `ProjectBuilder`, two
engine tests, CLI project creation, server template creation, and two server
tests that copy project builder files. This looks phaseable, but it should be
treated as the main M3 scope rather than a cleanup aside.

# Notes

- Planning M3 in parallel with M2.1 is acceptable if M3 phases avoid editing the
  same runtime-product files and defer final symbol integration until M2.1
  lands.
- `lpc-source` should not take a concrete `lpfs` dependency because that would
  create a crate cycle. Prefer source-owned generic traits, with implementations
  in `lpfs`.

## Q10: Should M3 switch wholesale from `node.json` to `node.toml`?

Context: The live Rust footprint is bounded but not tiny: legacy
loader/runtime reload, `ProjectBuilder`, engine update tests, CLI project
creation, server template creation, and server tests that copy project builder
files. A wholesale switch makes M3 larger, but avoids carrying an indefinite
compatibility loader and gives the milestone a clear completion point.

Suggested answer: Yes. Switch wholesale to `node.toml` in M3, with no long-term
`node.json` compatibility loader. Keep only migration notes and update tests,
builders, templates, and watcher logic in the same milestone.

Answer: Yes.

## Q11: Should examples be migrated as part of the wholesale switch?

Context: There are 20 live `examples/**/node.json` files. They appear to use the
current legacy config shapes and are referenced by firmware/test notes. If the
runtime stops loading `node.json`, leaving examples unchanged would make them
invalid and undermine the "finished" goal of the wholesale switch.

Suggested answer: Yes. Migrate live example project configs from `node.json` to
`node.toml` in M3. Update comments that refer to example `node.json` paths.
Archived docs and old plan files can remain historical.

Answer: Yes. These examples are actually used. Validate the converted perf
examples with the `lp-cli profile` perf tool so the new format is proven through
the same project push/load/tick path users rely on.

# Validation Notes

M3 should validate both ordinary tests and an end-to-end example workload:

- `cargo test -p lpc-source`
- `cargo test -p lpc-engine`
- `cargo test -p lpc-shared`
- `cargo test -p lpa-server`
- `cargo test -p lp-cli`
- `cargo run -p lp-cli -- profile examples/perf/fastmath --mode steady-render --max-cycles <bounded-cycle-budget> --collect events --note m3-node-toml`

The `lp-cli profile` command reads the local project directory, pushes all files
to the emulator-backed server, loads the project, and drives frames until the
profile gate or max-cycle cap stops the run. That makes it a useful validation
that the converted example source files are not merely syntactically valid but
load through the runtime path.
