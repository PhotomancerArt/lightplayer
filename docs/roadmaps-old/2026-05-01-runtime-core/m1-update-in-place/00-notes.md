# Scope of Work

This plan covers Milestone 1 of the runtime-core roadmap:
reorganize the legacy runtime work around an update-in-place strategy before
designing the final `Engine` API.

The milestone should:

- inventory the current `lpl-model`, `lpl-runtime`, and `lpc-engine::legacy_project`
  split;
- move the legacy split back into `lpc-*` crates as `legacy` modules;
- preserve existing `LegacyProjectRuntime` behavior and naming;
- identify the smallest compatibility slice for the next milestone;
- update roadmap documentation so future work follows the `lpc-*` update-in-place
  direction instead of introducing new domain crate families.

Out of scope:

- final `Engine` API design;
- pull-based bus/provider implementation;
- queryable visual outputs;
- porting all legacy nodes to the new `Node` trait;
- removing or renaming `LegacyProjectRuntime`;
- adding new visual node types.

# Current State

## Roadmap Direction

`docs/roadmaps/2026-05-01-runtime-core/notes.md` already leans toward folding
runtime/source/wire/model work into the existing `lpc-*` family and using modules
for legacy, visual, fixture, output, and future rig concepts. It explicitly calls
out the previous `lpl-*` / `lpv-*` direction as likely too much indirection while
the runtime owner contract is unsettled.

The legacy dataflow is already close to the desired pull behavior:

```text
fixture tick/render
  -> asks for texture
    -> lazy shader render for that texture
      -> shader writes texture buffer
  -> fixture samples texture
  -> fixture writes output buffer
output tick/render
  -> flush mutated output data
```

The immediate value is to simplify crate/module topology and protect that old
shader -> fixture -> output path while the new runtime owner is designed later.

## Legacy Model Crate

`lp-core/lpl-model` is a small `no_std` crate containing legacy node configs,
node state, project response/change types, and legacy wire message aliases:

- `NodeConfig`, `NodeKind`
- `NodeChange`, `NodeDetail`, `NodeState`, `ProjectResponse`
- `SerializableNodeDetail`, `SerializableProjectResponse`
- `LegacyMessage`, `LegacyServerMessage`, `LegacyServerMsgBody`

`lp-core/lpc-model/src/lib.rs` currently documents legacy node configs as living
in `lpl-model`, so moving legacy types will require doc and import updates.

Blast radius is broad. `lpl_model` appears in `lpc-engine`, `lpc-view`,
`lpc-shared`, `lpa-server`, `lpa-client`, firmware server loops/transports,
`fw-tests`, and `lp-cli` debug UI paths.

Decision from user feedback: do not keep `lpl-model` as a compatibility crate.
Eliminate `lpl-*` now and place legacy modules in existing `lpc-*` crates:

- legacy authored configs and source-facing specs go to `lpc-source::legacy`;
- legacy state, project responses, node changes/details, serializable response
  wrappers, and legacy message aliases go to `lpc-wire::legacy`;
- foundation-only legacy identifiers, if any are needed, can live under
  `lpc-model::legacy`, but avoid putting wire-coupled types in `lpc-model`;
- concrete legacy runtime/node implementations go to `lpc-engine::legacy`.

This keeps the dependency direction clean:

```text
lpc-model  <-  lpc-source
    ^             ^
    |             |
    +--------- lpc-wire
                    ^
                    |
                lpc-engine
```

`lpc-source` can depend on `lpc-model`; `lpc-wire` can depend on both
`lpc-model` and `lpc-source`; `lpc-engine` can depend on all three. `lpc-model`
should not depend on `lpc-source` or `lpc-wire`.

User confirmed that `lpc-wire` depending on `lpc-source` is acceptable because
source/config payloads can be sent over the wire.

## Legacy Runtime Crate

`lp-core/lpl-runtime` is a `no_std` crate with concrete legacy runtimes:

- `TextureRuntime`
- `ShaderRuntime`
- `FixtureRuntime`
- `OutputRuntime`
- `MemoryOutputProvider` and output traits/re-exports
- `project_hooks::install()`

`legacy_hooks.rs` implements `LegacyProjectRuntime` integration:

- `init_nodes` initializes in kind order: texture -> shader -> fixture -> output;
- `tick` advances frames and renders fixtures;
- fixture rendering lazily requests textures through `RenderContext`;
- texture rendering pulls shader outputs for the current frame;
- output rendering/flush happens after fixture mutation;
- filesystem changes update configs, shaders, and node presence;
- `get_changes` emits legacy `ProjectResponse` updates.

The file is intentionally legacy-heavy and already has documented `#[allow]`
attributes for deferred docs/size refactors. The plan should not add more
suppression, but it should undo the hook split by moving this integration back
into direct `LegacyProjectRuntime` methods.

## `lpc-engine` Legacy Boundary

`lp-core/lpc-engine` already owns `LegacyProjectRuntime`, the legacy loader,
`LegacyProjectHooks`, and `LegacyNodeRuntime`.

Important files:

- `lp-core/lpc-engine/src/legacy_project/project_runtime/core.rs`
- `lp-core/lpc-engine/src/legacy_project/project_runtime/types.rs`
- `lp-core/lpc-engine/src/legacy_project/legacy_loader.rs`
- `lp-core/lpc-engine/src/legacy_project/hooks.rs`
- `lp-core/lpc-engine/src/nodes/node_runtime.rs`
- `lp-core/lpc-engine/src/legacy_project/mod.rs`
- `lp-core/lpc-engine/src/lib.rs`

The hook boundary currently creates global registration indirection:
`LegacyProjectRuntime::{init_nodes,tick,handle_fs_changes,get_changes}` call
`legacy_project::hooks::with_hooks`, and callers must remember to run
`lpl_runtime::install()` before using node operations. This was introduced when
the old runtime was split into a core spine plus legacy runtime crate. For the
update-in-place direction, this indirection is now considered the wrong shape.

Git history confirms the hook mechanism came from
`da2f0a51 refactor(lpc-runtime/lpl-runtime): split lp-engine into spine + legacy
runtimes`, where the former monolithic `lp-engine::ProjectRuntime` methods were
split into `lpc-runtime::project::hooks` plus `lpl-runtime::legacy_hooks`.
Current `lpc-engine` already carries the shader/backend dependency stack, so
moving concrete legacy runtimes and legacy project operations back under
`lpc-engine` should not introduce a new category of dependency.

The plan should remove the hook mechanism rather than preserve or rename it:

- delete `LegacyProjectHooks`, `set_project_hooks`, and `with_hooks`;
- remove `lpl_runtime::install()` call sites;
- move or inline `legacy_hooks.rs` behavior so `LegacyProjectRuntime` directly
  implements `init_nodes`, `tick`, `handle_fs_changes`, and `get_changes`;
- keep `LegacyProjectRuntime` as the public runtime name.

There is doc drift: hook comments mention `lpc-runtime`, but the actual crate is
`lpc-engine`.

## Compatibility Surface

Names that appear public or cross-crate enough to preserve during this milestone:

- `LegacyProjectRuntime`
- `LegacyNodeRuntime`
- legacy config names such as `TextureConfig`, `ShaderConfig`, `FixtureConfig`,
  and `OutputConfig`, moved under `lpc_source::legacy`
- legacy runtime state/protocol names such as `NodeState`, `ProjectResponse`,
  `LegacyMessage`, and `LegacyServerMessage`, moved under `lpc_wire::legacy`
- `lpc_engine::legacy_project` re-exports

The plan should update import paths across the workspace in this milestone
rather than keeping `lpl-model` or `lpl-runtime` as compatibility crates.
`LegacyProjectHooks` and `lpl_runtime::install()` are implementation artifacts
of the split and should be removed, not preserved.

## Existing Validation Surface

Focused validation for reorganization steps:

```bash
cargo test -p lpc-engine
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

If a phase changes `legacy_hooks`, concrete node runtimes, shader compile/exec
paths, or firmware-visible type/message surfaces, add:

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

Per workspace rules, do not use `cargo build --workspace` or
`cargo test --workspace`.

# Questions

## Confirmation-Style Questions

| # | Question | Context | Suggested answer |
| --- | --- | --- | --- |
| Q1 | Where should legacy model/source/wire types move? | `lpl-model` mixes authored configs, runtime state, project responses, and wire message aliases. | Answered: configs/source specs to `lpc-source::legacy`; state/protocol/message types to `lpc-wire::legacy`; only foundation-only types to `lpc-model::legacy` if needed. |
| Q2 | Should the hook boundary be removed instead of preserved? | Git history shows the hook layer came from the split of the old monolithic runtime, and it now adds global registration complexity. | Answered: remove it and effectively revert that part of the split. |
| Q3 | Should `lpl-*` crates remain as compatibility shims? | Client/server/firmware/CLI imports currently use `lpl_model::*` and `lpl_runtime::install`, but the roadmap direction is to remove extra domain crates. | Answered: no, get rid of `lpl-*` now and update callers to `lpc-*::legacy` modules. |
| Q4 | Should the compatibility slice be shader + texture + fixture + output including lazy texture render and filesystem updates? | Existing tests cover render, shader edits, node config edits, deletion, and partial state updates. | Yes: this is the smallest useful old-flow slice. |
| Q5 | Should phase validation use `cargo test -p lpc-engine` plus `cargo check -p lpa-server` by default, with firmware checks only when runtime/compile paths move? | Full firmware validation is valuable but expensive; docs-only/import-only phases do not need it. | Yes. |

Answers:

- Q1: Config/source types move to `lpc-source::legacy`; state/protocol/message
  types move to `lpc-wire::legacy`; foundation-only types may use
  `lpc-model::legacy`.
- Q2: Remove the hook boundary.
- Q3: Do not keep `lpl-*` compatibility crates.
- Q4: Yes.
- Q5: Yes.

## Discussion-Style Questions

No discussion-style question is required yet if the remaining suggested answers
above are accepted. If any answer changes, update the design and phase
boundaries before writing phase files.

# Notes

- This milestone should probably be more documentation, module boundary, and
  compatibility planning plus a focused removal of the hook mechanism, not a
  broad rewrite of runtime behavior.
- The next milestone should be able to focus on the runtime owner/value-resolution
  contract rather than re-litigating `lpl-*` versus `lpc-*` topology.
- User answered Q2: the hook mechanism is overly complex and should be removed;
  use git history as evidence and effectively revert the hook split rather than
  preserving it.
- User clarified Q1/Q3: eliminate `lpl-model` and `lpl-runtime` now. Use
  `legacy` modules in `lpc-source`, `lpc-wire`, `lpc-engine`, and only
  `lpc-model` where a type is truly foundational.
- User confirmed that `lpc-wire` may depend on `lpc-source`.
