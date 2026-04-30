# M2 move map — concrete file targets per checkpoint

Companion to `m2-crate-restructure.md`. The milestone file says
"what" and "why"; this file says **exactly which file goes where**
so the RustRover moves can run uninterrupted. After each
checkpoint, ping the agent for import / `Cargo.toml` cleanup
before moving to the next.

End-state crate layout:

```
lp-core/
  lpc-model/        (foundation, on-wire types that don't reference legacy nodes)
  lpc-runtime/      (spine: ProjectRuntime, change events, gfx abstraction, output channels)
  lp-shared/        (unchanged — already exists)
  lp-server/        (unchanged location)
  lp-client/        (unchanged location)
  lp-engine-client/ (unchanged location)
lp-legacy/
  lpl-model/        (legacy node configs + node-aware on-wire types)
  lpl-runtime/      (TextureRuntime / ShaderRuntime / OutputRuntime / FixtureRuntime)
lp-vis/
  lpv-model/        (formerly lp-domain, visual types only — created in C4)
```

After M2: `lp-core/lp-model/`, `lp-core/lp-engine/`, and
`lp-domain/` no longer exist. All workspace crates match the
`lp{x}-` prefix convention.

## Quick rules

- **lpc** = "core", spine. **lpl** = "legacy". **lpv** = "visual".
- Anything that mentions `Texture` / `Shader` / `Output` /
  `Fixture` by *type name* is `lpl-`. Generic foundation
  (paths, frame ids, project handles, state machinery) is
  `lpc-`.
- **The protocol envelope was unbaked in a pre-M2 pass.**
  `Message<R>`, `ServerMessage<R>`, and `ServerMsgBody<R>` are
  now generic over the project-response payload, with type
  aliases (`LegacyMessage`, `LegacyServerMessage`,
  `LegacyServerMsgBody`) pinning `R = SerializableProjectResponse`.
  Result: the envelope itself is fully generic and goes to
  `lpc-model`. Only the *legacy-aware payload* — `NodeDetail`,
  `NodeState`, `SerializableNodeDetail`, `SerializableProjectResponse`,
  `ProjectResponse`, `NodeChange`, plus the legacy aliases — goes
  to `lpl-model`. lp-server / lp-client / fw-* depend on
  `lpc-model` for the envelope and `lpl-model` only when they
  touch the legacy aliases or the response payload.
- **`lp-engine-client`** uses the same protocol surface as
  `lp-server` and is a *runtime* (it holds a remote view) but
  its only changes for M2 are import updates — don't try to
  split it.

## C1 — split `lp-model` into `lpc-model` + `lpl-model`

### lpc-model — foundation + generic protocol envelope

Move these files from `lp-core/lp-model/src/` into
`lp-core/lpc-model/src/`:

- `path.rs` — `LpPath`, `LpPathBuf`, `AsLpPath`, `AsLpPathBuf`.
- `serial.rs` — `DEFAULT_SERIAL_BAUD_RATE`.
- `serde_base64.rs`.
- `transport_error.rs` — `TransportError`.
- `json.rs`.
- `config.rs` — `LightplayerConfig`.
- `state/` (whole directory) — `state_field.rs`, `macros.rs`,
  `test_state.rs`, `mod.rs`. Re-export `StateField` from
  `mod.rs`.
- `nodes/handle.rs` — `NodeHandle`.
- `nodes/specifier.rs` — `NodeSpecifier`.
- `project/handle.rs` — `ProjectHandle`.
- `project/config.rs` — `ProjectConfig`.
- `project/frame_id.rs` — `FrameId`.
- `server/config.rs` — `ServerConfig`.
- `server/fs_api.rs` — `FsRequest`, `FsResponse`.
- **`message.rs` (post-unbake)** — `Message<R>`, `ClientMessage`,
  `ServerMessage<R>`, `ClientRequest`, `NoDomain`. Now fully
  generic, lives in lpc-model.
- **`server/api.rs` (post-unbake)** — `ServerMsgBody<R>`,
  `ClientMsgBody`, `LogLevel`, `AvailableProject`,
  `LoadedProject`, `SampleStats`, `MemoryStats`. Now fully
  generic, lives in lpc-model.
- **From `project/api.rs`** — only the generic items move to
  lpc-model: `ProjectRequest`, `WireNodeSpecifier`,
  `NodeStatus`. The legacy-aware items
  (`NodeChange`, `NodeDetail`, `NodeState`, `SerializableNodeDetail`,
  `SerializableProjectResponse`, `ProjectResponse`) stay in
  lpl-model — see C1's lpl-model section. **This means
  `project/api.rs` needs to be split into two files** during
  the move: `project/api.rs` (lpc-model, generic items) and
  e.g. `project/legacy_api.rs` (lpl-model, legacy items).
  RustRover's "extract" can do this; alternatively the agent
  can do it as part of the cleanup.

Slim `nodes/mod.rs` for lpc-model (tracked here so the user
doesn't have to invent it):

```rust
pub mod handle;
pub mod specifier;

pub use handle::NodeHandle;
pub use specifier::NodeSpecifier;
```

Slim `project/mod.rs` for lpc-model:

```rust
pub mod api;
pub mod config;
pub mod frame_id;
pub mod handle;

pub use api::{WireNodeSpecifier, NodeStatus, ProjectRequest};
pub use config::ProjectConfig;
pub use frame_id::FrameId;
pub use handle::ProjectHandle;
```

Slim `server/mod.rs` for lpc-model:

```rust
pub mod api;
pub mod config;
pub mod fs_api;

pub use api::{
    AvailableProject, ClientMsgBody, LoadedProject, LogLevel,
    MemoryStats, SampleStats, ServerMsgBody,
};
pub use config::ServerConfig;
pub use fs_api::{FsRequest, FsResponse};
```

Slim `lib.rs` for lpc-model:

```rust
//! lpc-model: LightPlayer core data model — node-kind-agnostic
//! identity, addressing, paths, frame versioning, and the
//! generic protocol envelope.

#![no_std]

extern crate alloc;

pub mod config;
pub mod json;
pub mod message;
pub mod nodes;
pub mod path;
pub mod project;
pub mod serde_base64;
pub mod serial;
pub mod server;
pub mod state;
pub mod transport_error;

pub use config::LightplayerConfig;
pub use message::{ClientMessage, ClientRequest, Message, NoDomain, ServerMessage};
pub use nodes::{NodeHandle, NodeSpecifier};
pub use path::{AsLpPath, AsLpPathBuf, LpPath, LpPathBuf};
pub use project::{WireNodeSpecifier, FrameId, NodeStatus, ProjectConfig, ProjectHandle, ProjectRequest};
pub use serial::DEFAULT_SERIAL_BAUD_RATE;
pub use server::{ClientMsgBody, FsRequest, FsResponse, ServerConfig, ServerMsgBody};
pub use transport_error::TransportError;
```

### lpl-model — legacy node configs + legacy-aware payload types

Move these files from `lp-core/lp-model/src/` into
`lp-legacy/lpl-model/src/`:

- `glsl_opts.rs` — `GlslOpts`, `AddSubMode`, `MulMode`, `DivMode`.
- `nodes/kind.rs` — `NodeKind` (legacy enum with
  `Texture`/`Shader`/`Output`/`Fixture` variants).
- `nodes/{texture,shader,output,fixture}/` — whole subtrees:
  - `texture/{mod.rs, config.rs, state.rs, format.rs}`
  - `shader/{mod.rs, config.rs, state.rs}`
  - `output/{mod.rs, config.rs, state.rs}`
  - `fixture/{mod.rs, config.rs, state.rs, mapping.rs}`
- `nodes/mod.rs` — keep this file in lpl-model. Strip
  `pub mod handle;`, `pub mod specifier;` and the
  `pub use` of those, since they moved to lpc-model. Keep
  the `NodeConfig` trait, since it returns `NodeKind` (which
  is also legacy). Keep the per-kind `pub mod` declarations.
- **From `project/api.rs`** — only the legacy-aware items
  (NOT the generic ones, which moved to lpc-model):
  `NodeChange` (its `Created` variant uses `NodeKind`),
  `NodeDetail`, `NodeState`, `SerializableNodeDetail`,
  `SerializableProjectResponse`, `ProjectResponse`. Land
  these in `lpl-model/src/project/api.rs` (or a different
  file name — pick whatever's clean).
- **From `lib.rs`** — the legacy aliases that the unbake
  pass added to lp-model: `LegacyMessage`,
  `LegacyServerMessage`, `LegacyServerMsgBody`. Move them
  into `lpl-model/src/lib.rs` (where they belong long-term).

Slim `nodes/mod.rs` for lpl-model:

```rust
pub mod fixture;
pub mod kind;
pub mod output;
pub mod shader;
pub mod texture;

pub use kind::NodeKind;

use core::any::Any;

/// Node config trait - all legacy node configs implement this.
pub trait NodeConfig: core::fmt::Debug {
    fn kind(&self) -> NodeKind;
    fn as_any(&self) -> &dyn Any;
}
```

Slim `project/mod.rs` for lpl-model:

```rust
pub mod api;

pub use api::{
    NodeChange, NodeDetail, NodeState, ProjectResponse,
    SerializableNodeDetail, SerializableProjectResponse,
};
```

(No `server/` module under lpl-model — the protocol envelope
went to lpc-model in the unbake pass.)

Slim `lib.rs` for lpl-model:

```rust
//! lpl-model: legacy node configs and legacy-aware payload
//! types for LightPlayer 2025 (Texture / Shader / Output /
//! Fixture). The protocol envelope itself lives in lpc-model;
//! this crate provides the type aliases that pin the envelope
//! to the legacy response shape.

#![no_std]

extern crate alloc;

pub mod glsl_opts;
pub mod nodes;
pub mod project;

pub use nodes::{NodeConfig, NodeKind};
pub use project::{
    NodeChange, NodeDetail, NodeState, ProjectResponse,
    SerializableNodeDetail, SerializableProjectResponse,
};

pub type LegacyMessage = lpc_model::Message<SerializableProjectResponse>;
pub type LegacyServerMessage = lpc_model::ServerMessage<SerializableProjectResponse>;
pub type LegacyServerMsgBody = lpc_model::ServerMsgBody<SerializableProjectResponse>;
```

### Inside-the-files churn during C1

Within each file, replace `crate::path::...` → `lpc_model::path::...`
or just `lpc_model::...` etc. for the lpl-model files that
reference foundation types. RustRover should catch these via
its package-rename refactor; the agent will sweep for stragglers.

Cross-references to update once `project/api.rs` is split:

- `lpc-model/project/api.rs` (post-split): contains
  `ProjectRequest`, `WireNodeSpecifier`, `NodeStatus`. References
  only foundation types (`FrameId`, `NodeHandle`) — all already
  local.
- `lpc-model/server/api.rs` (post-move): the unbake removed
  `SerializableProjectResponse` from this file's imports. No
  downstream changes needed.
- `lpc-model/message.rs` (post-move): the unbake's
  `ClientRequest::ProjectRequest` references
  `project::api::ProjectRequest` which is local in lpc-model
  post-split.
- `lpl-model/project/api.rs` (the legacy-aware split):
  `crate::path::LpPathBuf` → `lpc_model::LpPathBuf`;
  `crate::project::FrameId` → `lpc_model::FrameId`;
  `crate::nodes::NodeHandle` → `lpc_model::NodeHandle`;
  `crate::project::api::{ProjectRequest, NodeStatus}` →
  `lpc_model::project::{ProjectRequest, NodeStatus}`. Other
  `crate::nodes::...` imports for `TextureConfig` etc. stay
  local.

### Cargo.toml — lpc-model

```toml
[package]
name = "lpc-model"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[features]
default = ["std"]
std = []
ser-write-json = ["dep:ser-write-json"]

[dependencies]
hashbrown = { workspace = true }
serde = { workspace = true, features = ["derive"] }
serde-json-core = { version = "0.6", default-features = false, features = ["custom-error-messages"] }
base64 = { workspace = true }
ser-write-json = { version = "0.3", optional = true, default-features = false, features = ["alloc"] }

[dev-dependencies]
tempfile = "3"

[lints]
workspace = true
```

### Cargo.toml — lpl-model

```toml
[package]
name = "lpl-model"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[features]
default = ["std"]
std = ["lpc-model/std"]
ser-write-json = ["lpc-model/ser-write-json"]

[dependencies]
lpc-model = { path = "../../lp-core/lpc-model", default-features = false }
hashbrown = { workspace = true }
serde = { workspace = true, features = ["derive"] }
serde-json-core = { version = "0.6", default-features = false, features = ["custom-error-messages"] }

[dev-dependencies]
tempfile = "3"

[lints]
workspace = true
```

### Workspace `Cargo.toml` updates after C1

Replace `"lp-core/lp-model"` in `members` and `default-members`
with **two entries**:

```
"lp-core/lpc-model",
"lp-legacy/lpl-model",
```

(`lp-core/lp-model` directory should be empty after the moves;
delete it.)

### Consumers needing import updates after C1

Per `rg "lp_model::"` results, these crates need either
`use lp_model::...` → `use lpc_model::...` or
`use lpl_model::...` (or both):

`lp-core/lp-server/`, `lp-core/lp-client/`, `lp-core/lp-engine/`,
`lp-core/lp-engine-client/`, `lp-core/lp-shared/`, `lp-cli/`,
`lp-fw/fw-core/`, `lp-fw/fw-emu/`, `lp-fw/fw-esp32/`,
`lp-fw/fw-tests/`, `lp-base/lpfs/`, `lp-domain/lp-domain/` (one
test file). Plus their `Cargo.toml` deps.

The agent does this sweep after C1.

## C2 — split `lp-engine` into `lpc-runtime` + `lpl-runtime`

### lpc-runtime — spine + graphics abstraction + output channels

Move these files from `lp-core/lp-engine/src/` into
`lp-core/lpc-runtime/src/`:

- `error.rs` — `Error` enum.
- `runtime/` (whole) — `contexts.rs` (`NodeInitContext`,
  `RenderContext`, `OutputHandle`, `TextureHandle`),
  `frame_time.rs`, `mod.rs`.
- `output/` (whole) — currently a re-export from `lp-shared`;
  keep the re-export shape, just move the file.
- `gfx/` (whole) — `lp_gfx.rs` (`LpGraphics`), `lp_shader.rs`
  (`LpShader`, `ShaderCompileOptions`), `uniforms.rs`,
  target-cfg backends (`host.rs`, `native_jit.rs`,
  `wasm_guest.rs`), `mod.rs`. The future lpfx roadmap will
  pull this out; for now it lives with the spine.
- `project/` (whole) — `loader.rs` (`discover_nodes`,
  `load_from_filesystem`, `load_node`), `project_runtime.rs`
  (`ProjectRuntime`, `NodeStatus`, `NodeEntry`,
  `MemoryStatsFn`), `mod.rs`.
- `nodes/node_runtime.rs` — the `NodeRuntime` trait. Keep it in
  lpc-runtime; lpl-runtime implements it but doesn't define it.

Slim `nodes/mod.rs` for lpc-runtime (just the trait — move the
per-kind submodules to lpl-runtime):

```rust
mod node_runtime;

pub use node_runtime::NodeRuntime;
```

Slim `lib.rs` for lpc-runtime:

```rust
//! lpc-runtime: LightPlayer spine — ProjectRuntime,
//! NodeRuntime trait, change events, frame versioning,
//! graphics abstraction, output channels.

#![no_std]

extern crate alloc;

pub mod error;
pub mod gfx;
pub mod nodes;
pub mod output;
pub mod project;
pub mod runtime;

pub use error::Error;
pub use gfx::{Graphics, LpGraphics, LpShader, ShaderCompileOptions};
pub use nodes::NodeRuntime;
pub use output::{MemoryOutputProvider, OutputChannelHandle, OutputFormat, OutputProvider};
pub use project::{MemoryStatsFn, ProjectRuntime};
pub use runtime::{NodeInitContext, RenderContext};
```

### lpl-runtime — legacy node implementations

Move these files from `lp-core/lp-engine/src/` into
`lp-legacy/lpl-runtime/src/`:

- `nodes/{fixture,output,shader,texture}/` — whole subtrees:
  - `fixture/` includes `mapping/` (sampling, overlap,
    structure, etc.) and `gamma.rs`.
- `nodes/mod.rs` — strip `mod node_runtime;` and `pub use
  node_runtime::NodeRuntime;` (those moved to lpc-runtime).
  Also strip `pub use lp_model::NodeConfig;`. The remainder
  is the per-kind submodule list:

```rust
pub mod fixture;
pub mod output;
pub mod shader;
pub mod texture;

pub use fixture::FixtureRuntime;
pub use output::OutputRuntime;
pub use shader::ShaderRuntime;
pub use texture::TextureRuntime;

pub use lpl_model::NodeConfig;
pub use lpc_runtime::NodeRuntime;
```

Slim `lib.rs` for lpl-runtime:

```rust
//! lpl-runtime: legacy node runtimes (Texture / Shader /
//! Output / Fixture) implementing the `lpc_runtime::NodeRuntime`
//! trait.

#![no_std]

extern crate alloc;

pub mod nodes;

pub use nodes::{FixtureRuntime, NodeConfig, NodeRuntime, OutputRuntime, ShaderRuntime, TextureRuntime};
```

### Inside-the-files churn during C2

Per-kind runtime files (`nodes/shader/runtime.rs` etc.)
currently use `crate::error::Error`, `crate::output::...`,
`crate::runtime::contexts::...`, `crate::gfx::...`, plus
`lp_model::nodes::shader::ShaderConfig` (etc). After C2 these
become:

- `crate::error::Error` → `lpc_runtime::Error`
- `crate::output::...` → `lpc_runtime::output::...`
- `crate::runtime::contexts::...` → `lpc_runtime::runtime::contexts::...`
- `crate::gfx::...` → `lpc_runtime::gfx::...`
- `lp_model::nodes::shader::ShaderConfig` → `lpl_model::nodes::shader::ShaderConfig` (unchanged structure, crate name change after C1).

`lpc-runtime`'s own internal references to `lp_model::...` are
all to lpc-model items per C1's split (paths, frame ids,
state machinery). RustRover should rename these.

`lpc-runtime/project/mod.rs` currently re-exports
`lp_model::project::api::{...}` — those types are in lpl-model
post-C1, so this re-export needs to move to lpl-runtime (or
just be deleted; consumers can `use lpl_model::...` directly).

### Cargo.toml — lpc-runtime

```toml
[package]
name = "lpc-runtime"
version.workspace = true
edition.workspace = true
license.workspace = true

[lints]
workspace = true

[features]
default = ["std"]
panic-recovery = ["dep:unwinding"]
std = [
    "lp-shared/std",
    "lpfs/std",
]

[dependencies]
unwinding = { version = "0.2", optional = true, default-features = false, features = ["panic"] }
serde = { workspace = true, features = ["derive"] }
hashbrown = { workspace = true }
log = { workspace = true, default-features = false }

lpc-model = { path = "../lpc-model", default-features = false }
lp-perf = { path = "../../lp-base/lp-perf", default-features = false }
lpfs = { path = "../../lp-base/lpfs", default-features = false }
lp-shared = { path = "../lp-shared", default-features = false }
lps-shared = { path = "../../lp-shader/lps-shared", default-features = false }

# Shader stack — used by gfx/* and project loader.
lpir = { path = "../../lp-shader/lpir", default-features = false }
lp-shader = { path = "../../lp-shader/lp-shader", default-features = false }
lpvm = { path = "../../lp-shader/lpvm", default-features = false }
lps-builtins = { path = "../../lp-shader/lps-builtins", default-features = false }
lps-frontend = { path = "../../lp-shader/lps-frontend", default-features = false }
lps-q32 = { path = "../../lp-shader/lps-q32", default-features = false }

libm = "0.2"

[target.'cfg(target_arch = "riscv32")'.dependencies]
lpvm-native = { path = "../../lp-shader/lpvm-native", default-features = false }

[target.'cfg(not(target_arch = "riscv32"))'.dependencies]
lpvm-wasm = { path = "../../lp-shader/lpvm-wasm", default-features = false }
```

(Note: `lpc-runtime` currently still depends on the shader
stack because `gfx/*` lives here. The lpfx roadmap moves that
out; for M2 it stays.)

### Cargo.toml — lpl-runtime

```toml
[package]
name = "lpl-runtime"
version.workspace = true
edition.workspace = true
license.workspace = true

[lints]
workspace = true

[features]
default = ["std"]
std = [
    "lpc-runtime/std",
    "lpl-model/std",
]

[dependencies]
serde = { workspace = true, features = ["derive"] }
hashbrown = { workspace = true }
log = { workspace = true, default-features = false }

lpc-model = { path = "../../lp-core/lpc-model", default-features = false }
lpc-runtime = { path = "../../lp-core/lpc-runtime", default-features = false }
lpl-model = { path = "../lpl-model", default-features = false }
lpfs = { path = "../../lp-base/lpfs", default-features = false }
lp-shared = { path = "../../lp-core/lp-shared", default-features = false }
lps-shared = { path = "../../lp-shader/lps-shared", default-features = false }

libm = "0.2"
```

### Workspace `Cargo.toml` updates after C2

Replace `"lp-core/lp-engine"` in `members` and `default-members`
with:

```
"lp-core/lpc-runtime",
"lp-legacy/lpl-runtime",
```

(`lp-core/lp-engine` directory should be empty after the moves;
delete it.)

### Consumers needing import updates after C2

Per `rg "lp_engine::"` results:

`lp-core/lp-server/` (project_manager, server, handlers,
template), `lp-core/lp-engine/tests/*.rs` (these tests move
into lpc-runtime or lpl-runtime — agent decides during cleanup),
`lp-core/lp-server/tests/*.rs`, plus the `Cargo.toml`s.

`lp-engine-client` doesn't import `lp_engine::` directly (per
grep), but its `Cargo.toml` references `lp-engine` as a
dev-dependency only.

## C3 — move `lpv-model` foundation into `lpc-model`

(C4 was completed out of order — see "C4 (DONE)" section
below. The crate is now `lpv-model` at `lp-vis/lpv-model/`.
C3 below is updated to reflect that.)

Move these files from `lp-vis/lpv-model/src/` into
`lp-core/lpc-model/src/`:

- `types.rs` (`Uid`, `Name`, `NodePath`, `PropPath`,
  `NodePropSpec`, `ArtifactSpec`, `ChannelName`).
- `kind.rs`, `constraint.rs`, `shape.rs` (includes `Slot`),
  `value_spec.rs`, `binding.rs`, `presentation.rs`.
- `artifact/` — whole directory (`Artifact`, `Migration`,
  `Registry` traits, `load.rs`).
- `schema/` — whole directory (versioning traits).
- `error.rs` — `DomainError`.
- `node/mod.rs` — **rename** the trait from `Node` to
  `NodeProperties` while moving (frees `Node` for the new
  tree-aware trait in M5).

The remainder of `lp-vis/lpv-model/src/` — `visual/` plus
`lib.rs`, `schema_gen_smoke.rs`, `examples/` — stays in place.

After moves, `lp-vis/lpv-model/src/lib.rs` becomes much
slimmer (no `pub mod artifact;`, etc.) and adds
`use lpc_model::{Slot, Kind, ...}` wherever `visual/*` needs
the foundation types. **No transitional re-export shell** —
visual code imports from `lpc_model` directly.

`lp-vis/lpv-model/Cargo.toml` adds `lpc-model = { path =
"../../lp-core/lpc-model", default-features = false }`.

## C4 — rename `lp-domain` → `lpv-model` under `lp-vis/` (DONE)

**Status:** completed out of order via `cargo-rename`
experiment, commit `f9a49014`. Single command:

```bash
cargo rename lp-domain lpv-model --move lp-vis/lpv-model
```

Manual fixes after: deleted empty `lp-domain/` parent
directory, updated stale doc-comment references in
`lp-vis/lpv-model/tests/round_trip.rs` (`cargo test -p
lp-domain` → `... -p lpv-model`).

Verified with `cargo check -p lpv-model`, `cargo test -p
lpv-model` (host + `std,schema-gen` features), `cargo check
-p fw-emu` and `-p fw-esp32` (RV32 release profiles).

Lessons applied to remaining checkpoints (see also "Workflow
note" at the bottom of this file).

## C5 — workspace polish (after agent does C1-C4 cleanup)

User actions:
- Cross-check that nothing accidentally still says `lp-model`
  or `lp-engine` (use the agent grep results to verify).
- Eliminate any compatibility shims left over from C1 / C2.
- Sanity-pass on per-crate `Cargo.toml` features.
- Fix anything the agent flagged as ambiguous during cleanup.

Agent actions (final gate):
- `just check` (fmt + clippy host + clippy rv32).
- `just build-ci` (host + rv32 builtins + emu-guest).
- `just test`.
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf
   --profile release-esp32 --features esp32c6,server`.
- `cargo check -p fw-emu --target riscv32imac-unknown-none-elf
   --profile release-emu`.

## Cleanup sweep checklist (agent runs after each checkpoint)

1. `rg "lp_model::"` — replace each match with `lpc_model::` or
   `lpl_model::` per the rules in this file.
2. `rg "lp_engine::"` — replace each match with `lpc_runtime::`
   or `lpl_runtime::`.
3. `rg "lp_domain::"` — after C4: replace each with
   `lpv_model::` or (for foundation types) `lpc_model::`.
4. `rg "lp-model"|"lp-engine"|"lp-domain"` in all `Cargo.toml`
   — update dep entries.
5. Verify per-crate slim `mod.rs` and `lib.rs` match the
   skeletons in this file (or are intentionally different).
6. `cargo check -p lpc-model` (after C1).
7. `cargo check -p lpl-model` (after C1).
8. `cargo check -p lpc-runtime` (after C2).
9. `cargo check -p lpl-runtime` (after C2).
10. `cargo check -p lp-server` — the load-bearing host
    consumer; if this passes, the M2 split is sound for the
    legacy stack.
11. `cargo check -p fw-esp32 --target
    riscv32imac-unknown-none-elf --profile release-esp32
    --features esp32c6,server` — target verification.

If any check fails after a cleanup sweep, the failure mode is
documented (which import wasn't found) before pinging the user
to continue.

## Workflow note: cargo-rename validated

C4 was executed by an agent using `cargo rename` and verified
end-to-end (commit `f9a49014`). The tool handled the rename
+ directory move atomically; manual fixes were limited to
two trivial things (empty parent dir cleanup + stale doc
comments in a test file). For the remaining checkpoints
(C1, C2, C3), the workflow is:

1. **Pure rename portion** — use `cargo rename <old> <new>
   --move <new-path>`. Always dry-run first.
2. **Split portion** (C1, C2 — extracting one crate into
   two) — agent does mechanical file moves: create the new
   crate skeleton, move per-kind subdirectories into it,
   author its `Cargo.toml`, update workspace members, sweep
   imports.
3. **Verify** — `cargo check` across host + RV32 targets
   per the cleanup-sweep checklist.
4. **Format with `cargo +nightly fmt -p <crate>`** scoped
   to affected crates rather than workspace-wide, to avoid
   touching unrelated formatting.
5. **Commit per-checkpoint** with conventional-commit
   format and a heredoc message.

Known cargo-rename limitations to grep for after every run:
- Doc comments referencing the old crate name (cargo-rename
  doesn't touch them — it only rewrites use/path/dep
  references).
- `include_str!` / `include_bytes!` paths.
- Build scripts (`build.rs`).
- Macro-expanded crate-name strings.
- Generic identifiers in unrelated crates that happen to
  share the old name (false positives — review dry-run).
