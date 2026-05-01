# Milestone 2: Crate restructure (mechanical, manual)

## Goal

Mechanically split `lp-core/lp-engine` and `lp-core/lp-model` into
the four-crate end-state: `lpc-model`, `lpc-engine`, `lpl-model`,
`lpl-runtime`. Move the foundation half of `lp-domain` into
`lpc-model`, then rename what remains of `lp-domain` to
`lpv-model` (under a new `lp-vis/` parent). ESP32 + emulator +
lp-cli stay green throughout. **No new concepts** introduced —
purely relocation, renames, import fixes.

This is the longest single chunk of work in the roadmap. It is
**user-driven in RustRover** (which has more powerful refactoring
than agent tools); the agent assists with import cleanup,
`Cargo.toml` polish, and verification after each checkpoint.

## Suggested plan location

`docs/roadmaps/2026-04-28-node-runtime/m2-crate-restructure/`

No plan file: this is direct execution with explicit checkpoints.
Scope and acceptance criteria for each checkpoint live in this
file.

## Scope

**In scope:**

- Type relocations (no semantic changes).
- Crate creation: `lpc-model`, `lpc-engine`, `lpl-model`,
  `lpl-runtime`, `lpv-model` (the last via rename of
  `lp-domain` after foundation moves out).
- `Cargo.toml` workspace updates; per-crate `Cargo.toml`
  with the same `no_std + alloc` and feature gating that
  `lp-engine` / `lp-model` / `lp-domain` use today.
- Re-exports / `pub use` so external consumers
  (`lp-server`, `lp-client`, `fw-esp32`, `fw-emu`,
  `lp-cli`, host tests) keep working with minimal churn.
- Import path fixes across the workspace.
- Existing tests preserved (relocated, not rewritten).
- `lp-domain` is dismantled: foundation types move to
  `lpc-model`; the remainder (visual types + their TOML
  examples) is renamed to `lp-vis/lpv-model/`.

**Out of scope:**

- New traits, slot views, `ArtifactManager`, lifecycle changes,
  any spine concepts. Those are M4 + M5.
- New tests beyond what's needed to verify the move.
- Touching `lpfx`, `lp-shader`, `lp-riscv`, firmware crates,
  emu-guest crates beyond updating their imports.
- Renaming or restructuring `lp-server` / `lp-client` /
  `lp-view` / `lp-cli` (only their imports change).
- Adding new types to `lpv-model`. M2 only moves what's
  already in `lp-domain` into its new home; refining the
  visual types is the next roadmap.

## Checkpoints

Mechanical scoping. Each checkpoint is a user-driven RustRover
move; agent verifies + cleans up before the next checkpoint.

### C1 — split `lp-model` into `lpc-model` + `lpl-model`

User action:

- Create `lp-core/lpc-model/` and `lp-legacy/lpl-model/`.
- Move generic foundation (handle / specifier / kind /
  config trait, the bits not specific to any node) into
  `lpc-model`.
- Move per-node configs (`nodes/texture/`, `nodes/shader/`,
  `nodes/output/`, `nodes/fixture/`) into `lpl-model`.
- `lp-model` becomes either deleted or a thin compatibility
  shim re-exporting from both (decision deferred to during
  the move).

Agent action after C1:

- Fix imports across the workspace.
- Update `Cargo.toml` deps.
- Verify `cargo check -p lp-server` + ESP32 + emulator.

### C2 — split `lp-engine` into `lpc-engine` + `lpl-runtime`

User action:

- Create `lp-core/lpc-engine/` and `lp-legacy/lpl-runtime/`.
- Move spine code into `lpc-engine`:
  `ProjectRuntime`, change events, `NodeStatus`, `FrameId`,
  fs-watch routing, panic recovery, shed plumbing,
  client / server protocol surface.
- Move per-node runtimes (`nodes/texture/`, `nodes/shader/`,
  `nodes/output/`, `nodes/fixture/`) into `lpl-runtime`.

Agent action after C2:

- Fix imports.
- Update `Cargo.toml` deps. `lpc-engine` deps on
  `lpc-model`. `lpl-runtime` deps on `lpl-model` +
  `lpc-engine` (for the trait it still implements via the
  *old* shape).
- Verify `cargo check -p lp-server` + ESP32 + emulator +
  `lp-cli`.

### C3 — move `lp-domain` foundation into `lpc-model`

User action:

- Move foundation types from `lp-domain/lp-domain/src/` into
  `lpc-model`:
  - `types.rs` (`Uid`, `Name`, `NodePath`, `PropPath`,
    `NodePropSpec`, `ArtifactSpec`, `ChannelName`).
  - `kind.rs`, `constraint.rs`, `shape.rs`, `slot.rs`,
    `value_spec.rs`, `binding.rs`, `presentation.rs`.
  - `artifact/` traits (`Artifact`, `Migration`, `Registry`).
  - `schema/` traits (versioning).
  - The existing `node::Node` property-access trait is
    **renamed** to `NodeProperties` to free the `Node` name
    for the new tree-aware trait that lands in M5.
- `lp-domain` keeps `visual/*` (Pattern / Effect / Stack /
  Transition / Live / Playlist / VisualInput / EffectRef /
  ParamsTable etc.) and imports foundation from `lpc-model`
  directly. **No transitional re-export shell** — visual
  types use `lpc_model::{Slot, Kind, ...}` from this
  checkpoint forward.

Agent action after C3:

- Fix imports.
- Verify `lp-domain` still parses its example TOMLs (still
  named `lp-domain` until C4).
- Verify `cargo check -p lp-server` + ESP32 + emulator.

### C4 — rename `lp-domain` → `lpv-model` under `lp-vis/`

User action:

- Create `lp-vis/` parent directory.
- Move `lp-domain/lp-domain/` (now visual-types-only) to
  `lp-vis/lpv-model/`.
- Rename the crate from `lp-domain` to `lpv-model` in
  `Cargo.toml`.
- Delete the now-empty `lp-domain/` parent directory.
- Move the `examples/v1/` TOMLs along with the crate; they
  belong with the visual model.

Agent action after C4:

- Fix all `use lp_domain::...` → `use lpv_model::...` across
  the workspace.
- Update `Cargo.toml` deps that named `lp-domain` to
  `lpv-model`.
- Verify `lpv-model` parses its example TOMLs unchanged.
- Verify `cargo check -p lp-server` + ESP32 + emulator +
  `lp-cli`.

### C5 — workspace polish

User action:

- Cross-check naming consistency.
- Eliminate compatibility shims left over from C1 / C2 if
  they're not needed.
- Sanity-pass on `Cargo.toml` features and target gating.

Agent action after C5:

- Run `just check` (fmt + clippy host + clippy rv32).
- Run `just build-ci` (host + rv32 builtins + emu-guest).
- Run `just test`.
- Confirm `cargo check -p fw-esp32 --target
  riscv32imac-unknown-none-elf --profile release-esp32
  --features esp32c6,server` is green.
- Confirm `cargo check -p fw-emu --target
  riscv32imac-unknown-none-elf --profile release-emu` is
  green.

## Key decisions

- **User does the type moves; agent does cleanup.** RustRover's
  refactoring catches dependents that grep-based tools miss.
- **Checkpoint cleanup is non-negotiable.** Each user → agent
  handoff has a green workspace as its acceptance criterion.
  No cumulative drift.
- **`lp-domain::node::Node` becomes `NodeProperties`.** The
  `Node` name is reserved for the new tree-aware trait that
  M5 introduces. Renaming early avoids a confusing two-trait
  state in M3 / M4.
- **`lp-domain` is dismantled in M2.** Foundation moves to
  `lpc-model` (C3); the visual-types-only remainder is
  renamed to `lp-vis/lpv-model/` (C4). After M2 every crate
  in the workspace matches the `lp{x}-` prefix convention.
  No outliers.
- **No transitional re-export shell.** Between C3 and C4,
  `lp-domain` imports foundation directly from `lpc-model`
  via `use lpc_model::...`. Don't bother shipping a
  short-lived `lp-domain` shell that just re-exports
  `lpc-model`.
- **No semantic changes in M2.** Even if a refactor is
  "obvious", it doesn't happen here. M3 / M4 / M5 own
  semantic changes.

## Deliverables

- `lpc-model`, `lpc-engine`, `lpl-model`, `lpl-runtime`,
  `lpv-model` crates exist and are workspace members.
- `lp-domain` no longer exists; `lp-vis/lpv-model/` is its
  successor (visual types only).
- `lp-engine` and `lp-model` either deleted or thin shims
  (decision: probably deleted — RustRover handles the rename
  cleanly and deps point straight at the new crates).
- Workspace gates green: `just check`, `just build-ci`,
  `just test`, ESP32 release build, emu release build.

## Dependencies

- None within this roadmap. Runs in parallel with M1.
- Blocks: M3 (design pass needs the new crate layout to
  reason about types living *in* the new homes).

## Execution strategy

**Option A — direct execution.** No `/plan` process.

Justification: M2 is mechanical relocation. The "design" of
where each type goes is already in this milestone file
(per-checkpoint scope). The user is doing the moves manually
in RustRover and pinging the agent for cleanup; that
interactive loop is the planning. A formal plan would just
restate the five checkpoints.

> I will execute this milestone directly. Each checkpoint is a
> user → agent handoff: user moves types in RustRover, then
> pings the agent to fix imports / `Cargo.toml` / verify gates
> before moving to the next checkpoint.
