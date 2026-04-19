# Phase 2 — `lpfx` parent crate reshape

`[sub-agent: yes, parallel: 1]`

## Scope of phase

Reshape the `lpfx` parent crate to match the new design from
`00-notes.md` (Q3, Q4, Q6) and `00-design.md`:

1. **Add a `std` Cargo feature** (default-on), forwarded to deps,
   mirroring `lp-engine`'s pattern. This unblocks RV32 builds for
   the eventual `lp-engine`-consumes-`lpfx` integration.
2. **Reshape the `FxInstance` trait** to take all uniforms per render
   call: drop `set_input`, change `render(&mut self, time: f32)` to
   `render(&mut self, inputs: &FxRenderInputs<'_>)`. Add a new
   `FxRenderInputs<'a>` struct.
3. **Drop the `format` parameter from `FxEngine::create_texture`** —
   only `Rgba16Unorm` is supported on CPU today (overview decision 5).
4. **Add a `defaults_from_manifest()` helper** so callers can seed
   their per-frame `Vec<(String, FxValue)>` from manifest defaults.
5. **Delete `CpuTexture` and `TextureFormat`** from `lpfx::texture`.
   The texture module shrinks to just `TextureId`. Their crate-root
   re-exports also disappear.

Phase 3 is the consumer of all of the above — phase 2 just lands the
trait + types + module changes and proves the parent crate alone
still compiles and tests pass.

## Out of scope

- Anything in `lpfx/lpfx-cpu/`. That crate will stop compiling after
  this phase (its `lib.rs` references `CpuTexture` and `set_input`);
  phase 3 fixes it. Do **not** edit `lpfx-cpu` from this phase.
- Anything in `examples/`. The `noise.fx` GLSL is phase 1's job.
- Anything outside `lpfx/lpfx/`.
- Adding new `FxValue` variants (Color, Palette, etc.) — not in M4c
  scope.
- Slot-based / `layout(binding = N)` uniform addressing — recorded as
  future work in `00-notes.md`. The slice-of-name-keyed-pairs shape is
  the M4c choice and will likely be replaced when slots come online.

## Code organization reminders

- One concept per file. Put `FxRenderInputs` in a new file
  `src/render_inputs.rs`; put `defaults_from_manifest` in a new file
  `src/defaults.rs`. Don't dump them into existing files.
- Tests near the top of each module (the existing files already
  follow this; keep it).
- `lib.rs` is the crate-root re-export hub — keep it just `pub use`s
  + module declarations + the crate-level `tests` mod.
- No `TODO` comments. Everything in this phase is the final shape.
- Keep `#![no_std]` + `extern crate alloc`. All new code uses
  `core` + `alloc` only.

## Sub-agent reminders

- Do **not** commit. Phase 4 commits the whole plan as one unit.
- Do **not** expand scope. Don't touch `lpfx-cpu`, `examples/`, or
  any other crate. The validation command checks `lpfx` only — that
  is intentional.
- Do **not** suppress warnings or add `#[allow(...)]`. Fix the
  underlying issue.
- Do **not** disable, `#[ignore]`, or weaken any test. The five
  tests in `lpfx/lpfx/src/lib.rs::tests` and three in
  `lpfx/lpfx/src/texture.rs::tests` (which will be deleted along
  with `CpuTexture`) are in scope.
- Do **not** add new public items beyond what this phase calls out.
- If anything is ambiguous or blocked, **stop and report**.
- Report back: files changed, validation output, and any deviations.

## Implementation details

### File 1 — `lpfx/lpfx/Cargo.toml`

Add a `std` feature (default-on). `lpfx`'s direct deps (`serde`,
`toml`) don't expose `std` knobs we need to forward — `serde` is
fine without `std` and `toml` is host-only and already pulls `std`
when active. The feature exists primarily so downstream crates
(`lpfx-cpu` and eventually `lp-engine`) can forward `std` through
`lpfx/std` and keep the per-crate forwarding pattern consistent
with `lp-engine`'s.

Replace the existing file body with:

```toml
[package]
name = "lpfx"
description = "LightPlayer effect modules: manifest types and parsing (no_std + alloc)"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[features]
default = ["std"]
std = []

[dependencies]
serde = { workspace = true, features = ["derive"] }
toml = { workspace = true }

[dev-dependencies]
lps-frontend = { path = "../../lp-shader/lps-frontend" }

[lints]
workspace = true
```

### File 2 (NEW) — `lpfx/lpfx/src/render_inputs.rs`

`FxRenderInputs` carries the full per-render uniform payload. Slice
form for `inputs` keeps allocations off the per-frame hot path; the
`'a` lifetime makes the borrow explicit.

```rust
//! Per-render uniform inputs for [`crate::engine::FxInstance::render`].

use crate::input::FxValue;

/// All inputs needed to render one frame.
///
/// Built per-call by the caller; not stored on the instance. Mirrors
/// `LpsPxShader::render_frame`'s shape (uniforms-per-call, no
/// per-instance uniform cache).
///
/// `time` is a typed field — the frame clock is mandatory. User-defined
/// manifest inputs go in `inputs` as `(&str, FxValue)` pairs; the
/// implementation looks each up by name and applies it to the
/// shader's `input_<name>` uniform.
pub struct FxRenderInputs<'a> {
    pub time: f32,
    pub inputs: &'a [(&'a str, FxValue)],
}
```

### File 3 (NEW) — `lpfx/lpfx/src/defaults.rs`

Caller-side helper to seed a `Vec<(String, FxValue)>` from manifest
defaults. The caller can then hand a slice into `FxRenderInputs.inputs`
and overlay any user-driven values.

```rust
//! Helper for seeding per-frame inputs from manifest defaults.

use alloc::string::String;
use alloc::vec::Vec;

use crate::input::FxValue;
use crate::manifest::FxManifest;

/// Collect every input from `manifest` that has a `default` value.
///
/// Inputs without a default are skipped; the caller supplies them
/// (or relies on the shader's own uniform initial value).
#[must_use]
pub fn defaults_from_manifest(manifest: &FxManifest) -> Vec<(String, FxValue)> {
    manifest
        .inputs
        .iter()
        .filter_map(|(name, def)| def.default.clone().map(|v| (name.clone(), v)))
        .collect()
}
```

### File 4 — `lpfx/lpfx/src/engine.rs`

Replace the file body. The trait changes:

- `FxEngine::create_texture` loses its `format` parameter
  (only `Rgba16Unorm` is supported on the CPU path; if we ever need
  another format we'll surface `lps_shared::TextureStorageFormat`
  directly rather than maintain a parallel `lpfx`-side enum).
- `FxInstance::set_input` is removed.
- `FxInstance::render` now takes `&FxRenderInputs<'_>` instead of
  `time: f32`.

```rust
//! Effect engine and per-instance runtime traits.

use crate::module::FxModule;
use crate::render_inputs::FxRenderInputs;
use crate::texture::TextureId;

/// Compiles effects and allocates output texture storage.
///
/// CPU backends only support `Rgba16Unorm` today; if a second format
/// is needed, surface `lps_shared::TextureStorageFormat` directly
/// rather than reintroducing a parallel `lpfx`-side enum.
pub trait FxEngine {
    type Instance: FxInstance;
    type Error: core::fmt::Display;

    /// Allocate a texture (`Rgba16Unorm`) and return an opaque handle.
    fn create_texture(&mut self, width: u32, height: u32) -> TextureId;

    fn instantiate(
        &mut self,
        module: &FxModule,
        output: TextureId,
    ) -> Result<Self::Instance, Self::Error>;
}

/// One runnable effect: render one frame, supplying all uniforms per call.
pub trait FxInstance {
    type Error: core::fmt::Display;

    /// Render one frame using the supplied inputs.
    ///
    /// `inputs.time` is the frame clock; `inputs.inputs` is a slice
    /// of `(name, value)` pairs matching manifest input names. The
    /// implementation maps each `name` to its `input_<name>`
    /// uniform.
    fn render(&mut self, inputs: &FxRenderInputs<'_>) -> Result<(), Self::Error>;
}
```

### File 5 — `lpfx/lpfx/src/texture.rs`

Shrink the file to just `TextureId`. Delete `CpuTexture`,
`TextureFormat`, all their methods, and the in-file `tests` mod
(those tests test the deleted types).

```rust
//! Opaque texture handles for effect outputs.

/// Opaque texture handle issued by [`crate::FxEngine::create_texture`](crate::engine::FxEngine::create_texture).
///
/// CPU backends use this as a key into their internal texture pool
/// (typically a `BTreeMap<TextureId, …>`). The actual pixel buffer
/// type is backend-specific and not surfaced through this trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureId(u32);

impl TextureId {
    /// Wrap a raw id. Backends allocate unique ids when creating textures.
    #[must_use]
    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }

    /// Raw id for maps and logging.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}
```

The `alloc::vec::Vec` import disappears with `CpuTexture`. So does
the entire `tests` mod at the bottom of the file.

### File 6 — `lpfx/lpfx/src/lib.rs`

Three changes:

1. Add `mod defaults;` and `mod render_inputs;` declarations.
2. Replace the texture re-export to drop `CpuTexture` and
   `TextureFormat`.
3. Add re-exports for `FxRenderInputs` and `defaults_from_manifest`.

Specifically, replace these two lines in the existing `lib.rs`:

```rust
pub use engine::{FxEngine, FxInstance};
…
pub use texture::{CpuTexture, TextureFormat, TextureId};
```

with:

```rust
pub use defaults::defaults_from_manifest;
pub use engine::{FxEngine, FxInstance};
pub use render_inputs::FxRenderInputs;
…
pub use texture::TextureId;
```

And add the two new module declarations alongside the existing ones:

```rust
pub mod engine;
mod error;
mod input;
mod manifest;
mod module;
mod parse;
pub mod texture;
mod defaults;
mod render_inputs;
```

(Module visibility: `defaults` and `render_inputs` are module-private;
their public API surface is the two `pub use`s.)

The existing `tests` mod at the bottom of `lib.rs` continues to
work — the five tests there only exercise manifest parsing, which is
unaffected.

## Validate

```bash
cargo build -p lpfx
cargo test  -p lpfx
```

The host build + tests are the bar. `lpfx-cpu` will fail to build at
the end of this phase — that is expected and is phase 3's job to fix.
**Do not** attempt to build any other crate from this phase.

If the `lpfx` build itself fails (e.g. an import that needs `core::`
instead of `std::` because we're `no_std`, a missing module decl, or
a forgotten test mod cleanup), **stop and report**.

When reporting back, include:

- The list of files added / edited / deleted.
- The output of `cargo build -p lpfx` and `cargo test -p lpfx`.
- The final diff of `lpfx/lpfx/src/texture.rs` and `lpfx/lpfx/src/engine.rs`
  so the parent agent can sanity-check the trait shape and texture
  module shrinkage.
