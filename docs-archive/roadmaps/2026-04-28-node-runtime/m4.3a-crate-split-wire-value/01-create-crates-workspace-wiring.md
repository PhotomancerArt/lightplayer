# Phase 1 — Create Crates + Workspace Wiring

## Scope of phase

Create the new `lp-core/lpc-source` and `lp-core/lpc-wire` crates and wire
them into the workspace with minimal compiling crate roots.

Out of scope:

- Do not move existing model/source/wire code yet.
- Do not change public APIs in `lpc-model` except what is necessary for the
  new crates to compile as empty shells.
- Do not rename existing non-core `lp-*` crates.
- Do not commit.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so it can be found later.

## Sub-agent reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If blocked by an unexpected workspace/dependency issue, stop and report.
- Report back: files changed, validation run, validation result, and any
  deviations from this phase.

## Implementation details

Add two new crates:

```text
lp-core/lpc-source/
├── Cargo.toml
└── src/
    └── lib.rs

lp-core/lpc-wire/
├── Cargo.toml
└── src/
    └── lib.rs
```

Both crates should be `#![no_std]` with `extern crate alloc;`.

`lpc-source` should start as the authored/source model crate:

```toml
[package]
name = "lpc-source"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[features]
default = ["std"]
std = ["lpc-model/std", "toml/std", "toml/preserve_order"]
schema-gen = ["std", "dep:schemars", "lpc-model/schema-gen"]

[dependencies]
lpc-model = { path = "../lpc-model", default-features = false }
serde = { workspace = true, features = ["derive"] }
toml = { version = "0.9", default-features = false, features = ["parse", "serde", "display"] }
schemars = { workspace = true, optional = true }

[dev-dependencies]
serde_json = { workspace = true }
toml = { workspace = true, features = ["display"] }

[lints]
workspace = true
```

`lpc-wire` should start as the engine/client wire model crate:

```toml
[package]
name = "lpc-wire"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[features]
default = ["std"]
std = ["lpc-model/std"]
schema-gen = ["std", "dep:schemars", "lpc-model/schema-gen"]

[dependencies]
lpc-model = { path = "../lpc-model", default-features = false }
serde = { workspace = true, features = ["derive"] }
schemars = { workspace = true, optional = true }

[dev-dependencies]
serde_json = { workspace = true }

[lints]
workspace = true
```

Minimal `src/lib.rs` shape:

```rust
//! LightPlayer core source model crate.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;
```

Use an equivalent crate doc for `lpc-wire`.

Update root `Cargo.toml`:

- Add `lp-core/lpc-source` and `lp-core/lpc-wire` to `[workspace].members`.
- Add both to `default-members`, near the other `lp-core` crates.
- Keep ordering readable and consistent with nearby entries.

Do not update downstream crates yet; later phases will add dependencies where
they are needed.

## Validate

Run:

```bash
cargo check -p lpc-model -p lpc-source -p lpc-wire
```

If formatting changed, run:

```bash
cargo +nightly fmt
```
