# Phase 2 — `lp-engine` backend selection refactor

`[sub-agent: yes, parallel: -]`

## Scope of phase

Replace `lp-engine`'s feature-flag-driven backend selection with
`cfg(target_arch = …)`-driven auto-selection, swapping the host
backend from Cranelift to Wasmtime in the same change.

End state inside `lp-engine`:

- One unqualified type `lp_engine::Graphics`. Selected by target:
  - `cfg(target_arch = "riscv32")` → wraps
    `lpvm-native::rt_jit::NativeJitEngine` (existing
    `gfx/native_jit.rs`).
  - `cfg(target_arch = "wasm32")` → wraps
    `lpvm-wasm::rt_browser::BrowserLpvmEngine` (new
    `gfx/wasm_guest.rs`).
  - catchall (host targets) → wraps
    `lpvm-wasm::rt_wasmtime::WasmLpvmEngine` (new `gfx/host.rs`).
- No backend-selection Cargo features on `lp-engine`. The
  `cranelift`, `cranelift-optimizer`, `cranelift-verifier`,
  `native-jit` features all disappear. `panic-recovery` and `std`
  stay.
- `lp-engine` no longer depends on `lpvm-cranelift`. It picks up
  `lpvm-native` (RV32) and `lpvm-wasm` (catchall + wasm32) via
  target-gated dep blocks.
- `gfx/cranelift.rs` is deleted.
- `gfx/native_jit.rs` is no longer feature-gated; the type is
  renamed `NativeJitGraphics` → `Graphics`.
- The intra-crate test in `nodes/shader/runtime.rs:445` loses its
  `feature = "cranelift"` cfg and uses `crate::Graphics`.
- `backend_name()` reports `"lpvm-native::rt_jit"` (RV32),
  `"lpvm-wasm::rt_wasmtime"` (catchall), `"lpvm-wasm::rt_browser"`
  (wasm32).

**Out of scope:**

- All consumers of `lp-engine` (`lp-server`, `lp-cli`, `fw-emu`,
  `fw-esp32`, integration tests outside `lp-engine`'s own tests).
  Those move in phase 3.
- `lpvm-cranelift` itself. It stays in the workspace; we just stop
  depending on it from `lp-engine`.
- `lpfx-cpu`. Has its own `cranelift` feature, unrelated to
  `lp-engine`'s; M4c handles it.
- Any behaviour change inside `gfx/native_jit.rs` beyond the type
  rename and dropping the feature gate.
- Any wasm32 smoke or runtime test. Phase 4 just `cargo check`s the
  wasm32 target; no `wasm-bindgen-test` or browser exec needed in M4b.

## Code organization reminders

- One concept per file. Each backend gets its own module under
  `gfx/`: `host.rs`, `wasm_guest.rs`, `native_jit.rs`.
- `gfx/mod.rs` is the dispatch table — keep it thin: `mod` decls,
  `pub use` re-exports, `pub use` of the trait. No logic.
- The `Graphics` type in each backend module is the public entry
  point (top of file). Helpers / shader wrapper structs sit at the
  bottom (mirror the existing layout in `cranelift.rs` /
  `native_jit.rs`).
- No drive-by changes to `gfx/lp_gfx.rs`, `gfx/lp_shader.rs`, or
  `gfx/uniforms.rs`. Their public surfaces stay.
- No `TODO`s for phase 3 work — that phase has its own file.

## Sub-agent reminders

- Do **not** commit. Phase 4 commits the whole plan.
- Do **not** expand scope. Don't touch `lp-server`, `lp-cli`, or any
  firmware crate from this phase — those are explicitly phase 3.
- Do **not** touch `gfx/native_jit.rs`'s body beyond renaming the
  type and removing the feature gate.
- Do **not** suppress warnings or add `#[allow(...)]`. Fix the
  underlying issue.
- Do **not** disable, `#[ignore]`, or weaken the
  `nodes/shader/runtime.rs` test.
- If the `lp-cli` / `lp-server` build at the end of this phase
  appears to break (which it will, until phase 3), confirm the
  failure is on those crates and **not** on `lp-engine` itself, then
  proceed. The validation command for this phase only checks
  `lp-engine`.
- If anything is ambiguous or blocked, **stop and report** — do not
  improvise.
- Report back: files changed, validation output, and any deviations
  from this phase file.

## Implementation details

### File 1 — `lp-shader/lpvm-wasm/Cargo.toml`

Add `lps-frontend` to the regular workspace deps, **only if** Phase
1 ended with the regression test in `tests/compile_with_config.rs`
referring to `lps_frontend` and the dev-dep already covers it. If
Phase 1 didn't change this file, leave it alone.

(Verification step only — most likely no edit needed.)

### File 2 — `lp-core/lp-engine/Cargo.toml`

Remove all backend-selection features and the `lpvm-cranelift` /
`lpvm-native` deps from the unconditional `[dependencies]` block.
Add target-gated blocks for the two backends. Keep `panic-recovery`,
`std`. The `lpvm-cranelift` dep goes away from `lp-engine`
entirely — `lp-engine` does not need it after the swap.

Full new `Cargo.toml` (replace the existing one verbatim):

```toml
[package]
name = "lp-engine"
version.workspace = true
edition.workspace = true
license.workspace = true

[lints]
workspace = true

[features]
default = ["std"]
# Panic recovery via catch_unwind (embedded targets)
panic-recovery = ["dep:unwinding"]
std = [
    "lp-shared/std",
]

[dependencies]
unwinding = { version = "0.2", optional = true, default-features = false, features = ["panic"] }
serde = { workspace = true, features = ["derive"] }
hashbrown = { workspace = true }
lpir = { path = "../../lp-shader/lpir", default-features = false }
lp-shader = { path = "../../lp-shader/lp-shader", default-features = false }
lpvm = { path = "../../lp-shader/lpvm", default-features = false }
lps-builtins = { path = "../../lp-shader/lps-builtins", default-features = false }
lps-frontend = { path = "../../lp-shader/lps-frontend", default-features = false }
lps-q32 = { path = "../../lp-shader/lps-q32", default-features = false }
log = { workspace = true, default-features = false }

lp-model = { path = "../lp-model", default-features = false }
lp-shared = { path = "../lp-shared", default-features = false }
lps-shared = { path = "../../lp-shader/lps-shared", default-features = false }
libm = "0.2"

# Backend selection: exactly one LPVM engine is wired into `gfx::Graphics`,
# chosen by target architecture rather than a Cargo feature.
#   - riscv32       → lpvm-native (rt_jit)        — bare-metal firmware
#   - everything else → lpvm-wasm (rt_wasmtime/rt_browser) — host or wasm32 guest
[target.'cfg(target_arch = "riscv32")'.dependencies]
# Note: lpvm-native's 'debug' feature (interleaved/disasm sections) is disabled
# for firmware builds to reduce code size and improve compilation performance.
lpvm-native = { path = "../../lp-shader/lpvm-native", default-features = false }

[target.'cfg(not(target_arch = "riscv32"))'.dependencies]
lpvm-wasm = { path = "../../lp-shader/lpvm-wasm", default-features = false }

[dev-dependencies]
lp-shared = { path = "../lp-shared", default-features = false, features = ["std"] }
lp-engine-client = { path = "../lp-engine-client", default-features = false, features = ["std"] }
env_logger = { workspace = true }
```

Notes:

- `lpvm-cranelift` removed from `[dependencies]`.
- `lpvm-native` moved into the `riscv32` target block.
- `lpvm-wasm` lives in the `not(target_arch = "riscv32")` block,
  which covers both the host catchall and the `wasm32-unknown-unknown`
  target (its own `Cargo.toml` selects `rt_wasmtime` vs `rt_browser`
  internally).
- The `cranelift`, `cranelift-optimizer`, `cranelift-verifier`,
  `native-jit` features are all gone.

### File 3 — `lp-core/lp-engine/src/gfx/mod.rs`

Replace the existing file body. New version dispatches by target
arch and re-exports a single `Graphics` type.

```rust
//! Graphics abstraction (`LpGraphics` / `LpShader`): boundary between the engine and shader backends.
//!
//! Backend selection is target-driven: exactly one `Graphics` impl is compiled
//! per target. There is no Cargo feature for picking a backend.
//!
//! | Target                                  | Module                | Backend                            |
//! |-----------------------------------------|-----------------------|------------------------------------|
//! | `cfg(target_arch = "riscv32")`          | [`native_jit`]        | `lpvm-native::rt_jit`              |
//! | `cfg(target_arch = "wasm32")`           | [`wasm_guest`]        | `lpvm-wasm::rt_browser`            |
//! | catchall (host)                         | [`host`]              | `lpvm-wasm::rt_wasmtime`           |

pub mod lp_gfx;
pub mod lp_shader;
pub(crate) mod uniforms;

#[cfg(target_arch = "riscv32")]
pub mod native_jit;
#[cfg(target_arch = "wasm32")]
pub mod wasm_guest;
#[cfg(not(any(target_arch = "riscv32", target_arch = "wasm32")))]
pub mod host;

pub use lp_gfx::LpGraphics;
pub use lp_shader::{LpShader, ShaderCompileOptions};

#[cfg(target_arch = "riscv32")]
pub use native_jit::Graphics;
#[cfg(target_arch = "wasm32")]
pub use wasm_guest::Graphics;
#[cfg(not(any(target_arch = "riscv32", target_arch = "wasm32")))]
pub use host::Graphics;
```

### File 4 (DELETE) — `lp-core/lp-engine/src/gfx/cranelift.rs`

Delete the file.

### File 5 (RENAME / EDIT) — `lp-core/lp-engine/src/gfx/native_jit.rs`

Two changes:

- Rename `NativeJitGraphics` → `Graphics` (type + `impl` blocks +
  `Default` impl).
- Update the file header doc-comment: drop the "feature `native-jit`
  enabled" sentence; replace with "Compiled when
  `cfg(target_arch = \"riscv32\")`."
- Update `backend_name()` from `"native-jit"` to
  `"lpvm-native::rt_jit"`.

Updated header:

```rust
//! RV32 native JIT backend for [`super::LpGraphics`] (`lpvm-native` `rt_jit`).
//!
//! Compiled when `cfg(target_arch = "riscv32")`. This is the only backend on
//! firmware targets (`fw-emu`, `fw-esp32`).
```

The internal helper struct `NativeJitShader` keeps its name — it's
file-local and `Graphics` is the public boundary.

### File 6 (NEW) — `lp-core/lp-engine/src/gfx/host.rs`

Wraps `lpvm-wasm::rt_wasmtime::WasmLpvmEngine` for host targets.
Mirrors `native_jit.rs` and `cranelift.rs` (now deleted) line by
line in shape. Use `WasmOptions::default()` as the engine's compile
options.

```rust
//! Host graphics backend (`lpvm-wasm` `rt_wasmtime`).
//!
//! Compiled on every target except `riscv32` and `wasm32`. Wraps
//! [`lpvm_wasm::rt_wasmtime::WasmLpvmEngine`], so all of LPIR → WASM →
//! wasmtime JIT happens in-process. Pre-grows linear memory once
//! per engine (see [`lpvm_wasm::WasmOptions::host_memory_pages`]) so
//! cached `LpvmBuffer` host pointers stay valid.

use alloc::boxed::Box;
use alloc::format;

use lp_shader::{LpsEngine, LpsPxShader, LpsTextureBuf};
use lps_shared::TextureBuffer;
use lpvm_wasm::WasmOptions;
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

use super::lp_gfx::LpGraphics;
use super::lp_shader::{LpShader, ShaderCompileOptions};
use crate::error::Error;
use crate::gfx::uniforms::build_uniforms;

/// Host shader graphics backed by `lpvm-wasm` + wasmtime.
pub struct Graphics {
    engine: LpsEngine<WasmLpvmEngine>,
}

impl Graphics {
    /// New host graphics with default `WasmOptions`.
    pub fn new() -> Self {
        let backend = WasmLpvmEngine::new(WasmOptions::default())
            .expect("WasmLpvmEngine::new with default WasmOptions");
        Self {
            engine: LpsEngine::new(backend),
        }
    }
}

impl Default for Graphics {
    fn default() -> Self {
        Self::new()
    }
}

impl LpGraphics for Graphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, Error> {
        let cfg = options.to_compiler_config();
        let px = self
            .engine
            .compile_px(source, lps_shared::TextureStorageFormat::Rgba16Unorm, &cfg)
            .map_err(|e| Error::Other {
                message: format!("{e}"),
            })?;
        Ok(Box::new(HostShader { px }))
    }

    fn backend_name(&self) -> &'static str {
        "lpvm-wasm::rt_wasmtime"
    }

    fn alloc_output_buffer(&self, width: u32, height: u32) -> Result<LpsTextureBuf, Error> {
        self.engine
            .alloc_texture(width, height, lps_shared::TextureStorageFormat::Rgba16Unorm)
            .map_err(|e| Error::Other {
                message: format!("alloc texture: {e:?}"),
            })
    }
}

struct HostShader {
    px: LpsPxShader,
}

impl LpShader for HostShader {
    fn render(&mut self, buf: &mut LpsTextureBuf, time: f32) -> Result<(), Error> {
        let uniforms = build_uniforms(buf.width(), buf.height(), time);
        self.px
            .render_frame(&uniforms, buf)
            .map_err(|e| Error::Other {
                message: format!("render_frame: {e}"),
            })
    }

    fn has_render(&self) -> bool {
        true
    }
}
```

Notes:

- `Graphics::new()` panics on engine construction failure. This
  matches `CraneliftGraphics::new()` / `NativeJitGraphics::new()`
  today (both are infallible). `WasmLpvmEngine::new` returns
  `Result` for `Engine::new` failures (essentially impossible with
  default `wasmtime::Config`) and the pre-grow step (will fail only
  if 1024 pages can't be allocated). If we want a fallible
  constructor later, add `try_new`; out of scope for M4b.

### File 7 (NEW) — `lp-core/lp-engine/src/gfx/wasm_guest.rs`

Same shape as `host.rs` but wrapping
`lpvm-wasm::rt_browser::BrowserLpvmEngine`. Exists so a
`cargo check --target wasm32-unknown-unknown -p lp-engine` works.

```rust
//! Wasm32 guest graphics backend (`lpvm-wasm` `rt_browser`).
//!
//! Compiled when `cfg(target_arch = "wasm32")`. Wraps
//! [`lpvm_wasm::rt_browser::BrowserLpvmEngine`] which runs the
//! emitted shader WASM via the host JS `WebAssembly.Module` /
//! `Instance` API.

use alloc::boxed::Box;
use alloc::format;

use lp_shader::{LpsEngine, LpsPxShader, LpsTextureBuf};
use lps_shared::TextureBuffer;
use lpvm_wasm::WasmOptions;
use lpvm_wasm::rt_browser::BrowserLpvmEngine;

use super::lp_gfx::LpGraphics;
use super::lp_shader::{LpShader, ShaderCompileOptions};
use crate::error::Error;
use crate::gfx::uniforms::build_uniforms;

/// Wasm32 guest shader graphics backed by `lpvm-wasm` + browser host.
pub struct Graphics {
    engine: LpsEngine<BrowserLpvmEngine>,
}

impl Graphics {
    /// New guest graphics with default `WasmOptions`.
    pub fn new() -> Self {
        let backend = BrowserLpvmEngine::new(WasmOptions::default())
            .expect("BrowserLpvmEngine::new with default WasmOptions");
        Self {
            engine: LpsEngine::new(backend),
        }
    }
}

impl Default for Graphics {
    fn default() -> Self {
        Self::new()
    }
}

impl LpGraphics for Graphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, Error> {
        let cfg = options.to_compiler_config();
        let px = self
            .engine
            .compile_px(source, lps_shared::TextureStorageFormat::Rgba16Unorm, &cfg)
            .map_err(|e| Error::Other {
                message: format!("{e}"),
            })?;
        Ok(Box::new(WasmGuestShader { px }))
    }

    fn backend_name(&self) -> &'static str {
        "lpvm-wasm::rt_browser"
    }

    fn alloc_output_buffer(&self, width: u32, height: u32) -> Result<LpsTextureBuf, Error> {
        self.engine
            .alloc_texture(width, height, lps_shared::TextureStorageFormat::Rgba16Unorm)
            .map_err(|e| Error::Other {
                message: format!("alloc texture: {e:?}"),
            })
    }
}

struct WasmGuestShader {
    px: LpsPxShader,
}

impl LpShader for WasmGuestShader {
    fn render(&mut self, buf: &mut LpsTextureBuf, time: f32) -> Result<(), Error> {
        let uniforms = build_uniforms(buf.width(), buf.height(), time);
        self.px
            .render_frame(&uniforms, buf)
            .map_err(|e| Error::Other {
                message: format!("render_frame: {e}"),
            })
    }

    fn has_render(&self) -> bool {
        true
    }
}
```

### File 8 — `lp-core/lp-engine/src/lib.rs`

Replace the conditional `pub use` block with a single unconditional
`pub use gfx::Graphics`. The trait re-exports stay.

Current:

```rust
pub use error::Error;
#[cfg(feature = "cranelift")]
pub use gfx::CraneliftGraphics;
#[cfg(all(target_arch = "riscv32", feature = "native-jit"))]
pub use gfx::NativeJitGraphics;
pub use gfx::{LpGraphics, LpShader, ShaderCompileOptions};
```

After:

```rust
pub use error::Error;
pub use gfx::{Graphics, LpGraphics, LpShader, ShaderCompileOptions};
```

### File 9 — `lp-core/lp-engine/src/nodes/shader/runtime.rs`

Update the test mod:

- Drop `feature = "cranelift"` from the cfg.
- Replace `crate::CraneliftGraphics::new()` with
  `crate::Graphics::new()`.

Before:

```rust
#[cfg(all(test, feature = "cranelift"))]
mod tests {
    use super::*;

    #[test]
    fn test_shader_runtime_creation() {
        let handle = lp_model::NodeHandle::new(0);
        let graphics: Arc<dyn LpGraphics> = Arc::new(crate::CraneliftGraphics::new());
        let runtime = ShaderRuntime::new(handle, graphics);
        let _boxed: alloc::boxed::Box<dyn NodeRuntime> = alloc::boxed::Box::new(runtime);
    }
}
```

After:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shader_runtime_creation() {
        let handle = lp_model::NodeHandle::new(0);
        let graphics: Arc<dyn LpGraphics> = Arc::new(crate::Graphics::new());
        let runtime = ShaderRuntime::new(handle, graphics);
        let _boxed: alloc::boxed::Box<dyn NodeRuntime> = alloc::boxed::Box::new(runtime);
    }
}
```

### `lp-engine/tests/*` — update to `Graphics`

These integration tests reference `lp_engine::CraneliftGraphics`:

- `lp-core/lp-engine/tests/scene_render.rs`
- `lp-core/lp-engine/tests/scene_update.rs`
- `lp-core/lp-engine/tests/partial_state_updates.rs`

In each, replace:

```rust
use lp_engine::{CraneliftGraphics, ...};
let graphics: Arc<dyn LpGraphics> = Arc::new(CraneliftGraphics::new());
```

with:

```rust
use lp_engine::{Graphics, ...};
let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
```

The `...` in the import is whatever was already there
(`LpGraphics`, `MemoryOutputProvider`, `ProjectRuntime`, etc.).

Don't otherwise touch these tests.

## Validate

```bash
# Host build of lp-engine (catchall path: gfx/host.rs).
cargo build -p lp-engine
cargo test  -p lp-engine

# RV32 cross-check (gfx/native_jit.rs path).
cargo check -p lp-engine --target riscv32imac-unknown-none-elf

# Wasm32 cross-check (gfx/wasm_guest.rs path).
cargo check -p lp-engine --target wasm32-unknown-unknown
```

`lp-server`, `lp-cli`, `fw-emu`, `fw-esp32` will *not* build at the
end of this phase — their `CraneliftGraphics` references and
feature flags still reference the deleted symbols. That's expected;
phase 3 fixes them. Do not attempt to build any other crate from
this phase.

If a build failure surfaces inside `lp-engine` itself (e.g. a stray
`#[cfg(feature = "cranelift")]` somewhere we missed, or an
`lpvm-native` import that `gfx/native_jit.rs` was implicitly
relying on through the workspace dep graph), **stop and report**.
