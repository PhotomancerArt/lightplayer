# Phase 01 — Delete mockup and bootstrap crate

**Dispatch:** [sub-agent: yes, model: composer-2-fast, parallel: -]

## Scope of phase

- Delete `lp-core/lpc-slot-mockup/` entirely.
- Remove `"lp-core/lpc-slot-mockup"` from workspace `Cargo.toml` `members`.
- Create `lp-core/lpc-node-registry/` with `Cargo.toml`, `src/lib.rs`, and **empty
  stub modules** for future milestones.
- Crate must compile (`cargo check -p lpc-node-registry`).

**Out of scope:** `artifact/` implementation (phase 02+). Do not touch `lpc-engine`.

## Code Organization Reminders

- One concept per file; entry points at top of each file, helpers at bottom.
- Tests at bottom of files in `#[cfg(test)] mod tests`.
- Stub modules get a one-line `//! M2` (etc.) doc comment only.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** expand scope.
- Do **not** suppress warnings with `#[allow]` — fix them.
- Do **not** disable or weaken existing tests.
- If blocked, stop and report back.

## Implementation Details

### Delete mockup

Remove directory `lp-core/lpc-slot-mockup/`. Grep workspace for
`lpc-slot-mockup` and remove references (expect only root `Cargo.toml` members).

### `lp-core/lpc-node-registry/Cargo.toml`

```toml
[package]
name = "lpc-node-registry"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[features]
default = ["std"]
std = ["lpc-model/std", "lpfs/std"]

[dependencies]
lpc-model = { path = "../lpc-model", default-features = false }
lpfs = { path = "../../lp-base/lpfs", default-features = false }

[lints]
workspace = true
```

Add `"lp-core/lpc-node-registry"` to workspace `members` in root `Cargo.toml`
(near other `lp-core/*` entries). Add to `default-members` as well (host CI).

### `src/lib.rs`

```rust
#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;

mod registry;
mod source;
mod change;
mod view;
```

Stub files (`src/registry/mod.rs`, etc.):

```rust
//! NodeDefRegistry — milestone M2.
```

`src/artifact/mod.rs` — empty module declaration only for now (phase 02 fills it):

```rust
// Types added in phase 02.
```

Or minimal placeholder so `pub mod artifact` compiles.

### Workspace

Ensure `cargo check -p lpc-node-registry` succeeds with no warnings.

## Validate

```bash
cargo check -p lpc-node-registry
rg "lpc-slot-mockup" Cargo.toml lp-core/ --glob '!docs/**' || true
```

Expected: no mockup dir; registry crate compiles.
