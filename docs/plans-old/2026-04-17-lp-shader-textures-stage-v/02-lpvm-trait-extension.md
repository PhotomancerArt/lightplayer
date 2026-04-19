# Phase 2 — `LpvmInstance::call_render_texture` on every backend

## Scope

Add the `call_render_texture` trait method to `LpvmInstance` and
implement it on all six backends. The method **resolves the entry by
name on first call, caches the resolved entry internally, and reuses
the cache on subsequent calls** so the per-frame cost is one direct
call (no name lookup, no allocation, no marshalling).

This phase delivers no end-user-visible feature on its own; the synth
and adapter (Phases 3–4) consume the trait method. This phase ships
with a single unit test on the lpvm-cranelift impl as the JIT smoke
(Q10 #5) — the other backends are validated by `cargo build` here and
exercised end-to-end in Phase 5.

Closes Q8, Q12, Q13 in [`00-notes.md`](./00-notes.md).

## Code organisation reminders

- The trait method uses concrete types (`&mut LpvmBuffer`, `u32`,
  `u32`), not the generic `&[i32]` slice — that's the whole point.
- Each backend owns its own cache shape; the trait stays free of
  associated types.
- The `Pointer` argument extraction differs per backend:
  - `lpvm-cranelift`: `texture.native_ptr() as i64` (real host ptr).
  - All others: `texture.guest_base() as i32` (32-bit guest offset).
- Signature validation runs **on first lookup only** — verify the
  resolved function has shape `(Pointer, I32, I32) -> ()`. Reuse the
  backend's existing `Error` type; "function not found" / "signature
  mismatch" already exist as variants on every backend.

## Implementation details

### `lp-shader/lpvm/src/instance.rs`

Add the new method to the `LpvmInstance` trait. Place it after
`call_q32` (around line 53) so related ABI methods stay grouped:

```rust
/// Hot path: invoke a synthesised `__render_texture[_<format>]`
/// entry by name. The instance resolves the entry on first call,
/// caches it internally, and reuses the cache on subsequent calls
/// — so per-frame cost is one direct machine call.
///
/// Validates signature shape `(Pointer, I32, I32) -> ()` on the
/// first lookup. Returns the backend's existing `Error` type for
/// missing symbol, signature mismatch, or guest trap.
///
/// `texture` carries both the host pointer (used by the JIT host
/// backend) and the 32-bit guest offset (used by RV32 / emu /
/// WASM); each backend extracts what its calling convention needs.
fn call_render_texture(
    &mut self,
    fn_name: &str,
    texture: &mut lpvm::LpvmBuffer,
    width: u32,
    height: u32,
) -> Result<(), Self::Error>;
```

(`LpvmBuffer` is already in scope via `use lpvm::LpvmBuffer;` after
adding the import. It's defined in [`lp-shader/lpvm/src/buffer.rs`](../../../lp-shader/lpvm/src/buffer.rs).)

### Shared validation helper (recommended)

Each backend needs to validate the resolved function has signature
`(Pointer, I32, I32) -> ()`. Validate against the **`IrFunction`**
parameter types (which use `IrType::Pointer` directly) rather than
the surface-level `LpsFnSig` types — this avoids needing a new
`LpsType::Pointer` variant (which would touch ~20 exhaustive match
sites across the workspace; see Phase 3 for the rationale).

Factor into a small helper in `lpvm/src/lib.rs`:

```rust
use lpir::{IrFunction, IrType};

/// Verify an IrFunction has the shape required by `call_render_texture`:
///   (Pointer, I32, I32) -> ()
pub fn validate_render_texture_sig_ir(ir: &IrFunction) -> Result<(), &'static str> {
    if !ir.return_types.is_empty() {
        return Err("render-texture function must return void");
    }
    if ir.param_count != 3 {
        return Err("render-texture function must take 3 parameters");
    }
    // vreg_types[0] is vmctx (always Pointer); user params start at index 1.
    let p0 = ir.vreg_types.get(1).copied();
    let p1 = ir.vreg_types.get(2).copied();
    let p2 = ir.vreg_types.get(3).copied();
    if p0 != Some(IrType::Pointer) {
        return Err("render-texture param 0 must be Pointer");
    }
    if p1 != Some(IrType::I32) {
        return Err("render-texture param 1 must be I32 width");
    }
    if p2 != Some(IrType::I32) {
        return Err("render-texture param 2 must be I32 height");
    }
    Ok(())
}
```

(Confirm during implementation that `IrFunction` exposes
`return_types`, `param_count`, and `vreg_types` directly — they're
all visible in [`lpir/src/lpir_module.rs:37`](../../../lp-shader/lpir/src/lpir_module.rs)
based on the FunctionBuilder write-out path. Adjust accessors if
the public API differs.)

> The matching `LpsFnSig.parameters[0].ty` slot will use
> `LpsType::UInt` (1 scalar word, satisfies the ABI machinery)
> with a documented "synthetic — see kind" convention. Phase 3
> details this. The truth-of-record for "is this a pointer" lives
> in `IrFunction.vreg_types`, which the validator above consults
> directly.

### `lp-shader/lpvm-cranelift/src/lpvm_instance.rs`

Add to `CraneliftInstance`:

```rust
struct RenderTextureEntry {
    name: String,
    code: *const u8,
}

pub struct CraneliftInstance {
    // existing fields ...
    render_texture_cache: Option<RenderTextureEntry>,
}
```

Initialise `render_texture_cache: None` in `CraneliftInstance::new`.

Implementation in the `impl LpvmInstance for CraneliftInstance` block
(after `call_q32`, around line 324):

```rust
fn call_render_texture(
    &mut self,
    fn_name: &str,
    texture: &mut LpvmBuffer,
    width: u32,
    height: u32,
) -> Result<(), Self::Error> {
    if self.module.float_mode() != FloatMode::Q32 {
        return Err(InstanceError::Unsupported(
            "CraneliftInstance::call_render_texture requires FloatMode::Q32",
        ));
    }

    let code = self.resolve_render_texture(fn_name)?;

    // Hot path: vmctx + (host_ptr: i64, width: i32, height: i32).
    // Signature placement matches signature_for_ir_func: vmctx is the
    // first explicit param when no struct-return. For void returns, no
    // sret slot.
    let vmctx = self.vmctx_ptr() as usize;
    let tex_ptr = texture.native_ptr();

    unsafe {
        type RenderFn = extern "C" fn(usize, *mut u8, i32, i32);
        let f: RenderFn = core::mem::transmute(code);
        f(vmctx, tex_ptr, width as i32, height as i32);
    }
    Ok(())
}
```

`resolve_render_texture` is a private helper that does the cache check
+ validation:

```rust
impl CraneliftInstance {
    fn resolve_render_texture(
        &mut self,
        fn_name: &str,
    ) -> Result<*const u8, InstanceError> {
        if let Some(entry) = &self.render_texture_cache {
            if entry.name == fn_name {
                return Ok(entry.code);
            }
        }

        // Look up the IrFunction + validate shape
        let ir_fn = self.module.ir_function(fn_name).ok_or_else(|| {
            InstanceError::Call(CallError::MissingMetadata(fn_name.into()))
        })?;
        lpvm::validate_render_texture_sig_ir(ir_fn)
            .map_err(|e| InstanceError::Call(CallError::Unsupported(format!(
                "render-texture sig invalid: {e}"
            ))))?;

        let code = self.module.code_ptr(fn_name).ok_or_else(|| {
            InstanceError::Call(CallError::Unsupported(format!(
                "render-texture entry `{fn_name}` not in JIT image"
            )))
        })?;

        self.render_texture_cache = Some(RenderTextureEntry {
            name: fn_name.into(),
            code,
        });
        Ok(code)
    }
}
```

> Notes for this impl:
> - `vmctx` placement: confirm against `signature_for_ir_func` —
>   when there's no struct return, the order is `(vmctx, user_args...)`.
>   For our void-return `__render_texture`, that's
>   `(vmctx, tex_ptr, width, height)`.
> - `Pointer` argument: Cranelift's `IrType::Pointer` lowers to
>   `types::I64` on 64-bit hosts. The `extern "C" fn` signature uses
>   `*mut u8` to match.
> - Caching strategy: `Option<(String, *const u8)>` is fine since
>   one instance typically renders one format. If we ever need
>   multi-format-per-instance the cache can grow into a
>   `BTreeMap<String, *const u8>` without trait-level changes.

### `lp-shader/lpvm-native/src/rt_jit/instance.rs`

Add to `NativeJitInstance`:

```rust
struct RenderTextureEntry {
    name: String,
    entry_pc: usize,        // resolved RV32 entry address (host-mapped)
}

pub struct NativeJitInstance {
    // existing fields ...
    render_texture_cache: Option<RenderTextureEntry>,
}
```

Initialise to `None` in the constructor.

Implementation:

```rust
fn call_render_texture(
    &mut self,
    fn_name: &str,
    texture: &mut LpvmBuffer,
    width: u32,
    height: u32,
) -> Result<(), Self::Error> {
    if self.module.inner.options.float_mode != FloatMode::Q32 {
        return Err(NativeError::Call(CallError::Unsupported(String::from(
            "NativeJitInstance::call_render_texture requires FloatMode::Q32",
        ))));
    }

    let entry = self.resolve_render_texture(fn_name)?;

    // RV32 ABI: a0 = vmctx_guest, a1 = tex_ptr_guest, a2 = width, a3 = height.
    // Pointer is 32-bit on RV32 — extract guest_base from the buffer.
    let tex_offset = i32::try_from(texture.guest_base()).map_err(|_| {
        NativeError::Call(CallError::Unsupported(format!(
            "texture guest base {:#x} exceeds i32 range", texture.guest_base()
        )))
    })?;
    let vmctx = self.vmctx_guest as i32;

    #[cfg(target_arch = "riscv32")]
    unsafe {
        crate::rt_jit::call::rv32_jalr_a0_a7(
            entry,
            vmctx,
            tex_offset,
            width as i32,
            height as i32,
            0, 0, 0, 0,
        );
    }
    #[cfg(not(target_arch = "riscv32"))]
    {
        let _ = (entry, vmctx, tex_offset, width, height);
        return Err(NativeError::Call(CallError::Unsupported(String::from(
            "NativeJitInstance::call_render_texture requires riscv32 host",
        ))));
    }

    Ok(())
}
```

> The `cfg(target_arch = "riscv32")` gate matches the existing
> `rv32_jalr_a0_a7` definition in
> [`rt_jit/call.rs`](../../../lp-shader/lpvm-native/src/rt_jit/call.rs).
> Existing `invoke_flat` is also gated this way (look around the
> `pack_regs_direct` call — confirm during implementation).

`resolve_render_texture` mirrors the Cranelift helper but uses
`module.entry_offset(name)` and `module.buffer().entry_ptr(off)` to
get the entry, exactly like `invoke_flat` does today
([`rt_jit/instance.rs:188-192`](../../../lp-shader/lpvm-native/src/rt_jit/instance.rs)).

### `lp-shader/lpvm-native/src/rt_emu/instance.rs`

The emu path doesn't JIT — it runs the RV32 image through an
emulator. The "cache" is the resolved entry-pc into the emulator's
program image; same lookup strategy as `call_q32` in
[`rt_emu/instance.rs:115`](../../../lp-shader/lpvm-native/src/rt_emu/instance.rs).

```rust
fn call_render_texture(
    &mut self,
    fn_name: &str,
    texture: &mut LpvmBuffer,
    width: u32,
    height: u32,
) -> Result<(), Self::Error> {
    let entry_pc = self.resolve_render_texture(fn_name)?;

    let tex_offset = i32::try_from(texture.guest_base()).map_err(|_| /* … */)?;
    let vmctx = self.vmctx_guest as i32;

    // Set a0..a3, run the emulator until ret. The exact API is the same
    // path call_q32 uses — refactor invoke_flat or add a thin direct-call
    // sibling that takes (entry_pc, &[a0,a1,a2,a3]).
    self.run_emu_at(entry_pc, &[vmctx, tex_offset, width as i32, height as i32], 0 /* n_ret */)?;
    Ok(())
}
```

> The emu's existing `invoke_flat` (`rt_emu/instance.rs:115`) takes a
> `&str` and re-resolves every time. For this phase, factor the
> "(entry_pc, &[args]) → run + read returns" piece out of
> `invoke_flat` into a private `run_emu_at(entry_pc, args, n_ret)` so
> both `invoke_flat` and `call_render_texture` call it.

### `lp-shader/lpvm-emu/src/instance.rs`

Same shape as `rt_emu`. Cache an `entry_pc: usize`; call
`self.run_at(entry_pc, &[vmctx, tex_offset, width, height])` (refactor
out of the existing `call_q32` body around line 275 in
[`lpvm-emu/src/instance.rs`](../../../lp-shader/lpvm-emu/src/instance.rs)).

### `lp-shader/lpvm-wasm/src/rt_wasmtime/instance.rs`

Cache the resolved `wasmtime::Func`:

```rust
struct RenderTextureEntry {
    name: String,
    func: wasmtime::Func,
}

pub struct WasmLpvmInstance {
    // existing fields ...
    render_texture_cache: Option<RenderTextureEntry>,
}
```

Implementation:

```rust
fn call_render_texture(
    &mut self,
    fn_name: &str,
    texture: &mut LpvmBuffer,
    width: u32,
    height: u32,
) -> Result<(), Self::Error> {
    if self.float_mode != FloatMode::Q32 {
        return Err(WasmError::runtime(
            "WasmLpvmInstance::call_render_texture requires FloatMode::Q32",
        ));
    }

    let func = self.resolve_render_texture(fn_name)?;
    let tex_offset = i32::try_from(texture.guest_base()).map_err(|_| /* … */)?;

    let mut guard = self.runtime.lock();
    self.reset_globals_with_guard(&mut guard);
    self.prepare_call(&mut guard.store, guard.memory)?;

    let store = &mut guard.store;
    func.call(
        &mut *store,
        &[Val::I32(tex_offset), Val::I32(width as i32), Val::I32(height as i32)],
        &mut [],
    )
    .map_err(|e| WasmError::runtime(format!("WASM trap: {e}")))?;
    Ok(())
}
```

`resolve_render_texture` does the cache check and pulls
`wasmtime::Instance::get_func` (mirroring
[`rt_wasmtime/instance.rs:287-290`](../../../lp-shader/lpvm-wasm/src/rt_wasmtime/instance.rs)).

> WASM uses the linear-memory base internally; the WASM module
> already knows its memory base, so we don't need to pass `vmctx`
> explicitly. (Confirm: `call_q32` doesn't pass vmctx as an arg
> either — it just passes user args. The vmctx pointer is set up
> via `prepare_call`.)

### `lp-shader/lpvm-wasm/src/rt_browser/instance.rs`

Mirrors the wasmtime path but goes through `js_sys::Function::apply`
or `Reflect::apply`. Cache the resolved `js_sys::Function`:

```rust
struct RenderTextureEntry {
    name: String,
    func: js_sys::Function,
}
```

Look-up uses `Reflect::get(&self.exports_obj, &JsValue::from_str(name))`
and casts to `Function` (mirroring
[`rt_browser/instance.rs:149-151`](../../../lp-shader/lpvm-wasm/src/rt_browser/instance.rs)).
Call with three `JsValue::from(i32)` args.

### Tests added in this phase

**Trait + helper unit tests:**

```rust
// lp-shader/lpvm/src/lib.rs (test module)

fn make_ir_fn_with_param_types(name: &str, params: &[IrType], rets: &[IrType]) -> IrFunction {
    use lpir::builder::FunctionBuilder;
    let mut fb = FunctionBuilder::new(name, rets);
    for ty in params { fb.add_param(*ty); }
    fb.push_return(&[]);
    fb.finish()
}

#[test]
fn validate_render_texture_sig_ir_accepts_expected() {
    let f = make_ir_fn_with_param_types(
        "__render_texture_rgba16",
        &[IrType::Pointer, IrType::I32, IrType::I32],
        &[],
    );
    assert!(validate_render_texture_sig_ir(&f).is_ok());
}

#[test]
fn validate_render_texture_sig_ir_rejects_wrong_return() {
    let f = make_ir_fn_with_param_types("bad", &[IrType::Pointer, IrType::I32, IrType::I32], &[IrType::I32]);
    assert!(validate_render_texture_sig_ir(&f).is_err());
}

#[test]
fn validate_render_texture_sig_ir_rejects_wrong_arity() {
    let f = make_ir_fn_with_param_types("bad", &[IrType::Pointer, IrType::I32], &[]);
    assert!(validate_render_texture_sig_ir(&f).is_err());
}

#[test]
fn validate_render_texture_sig_ir_rejects_non_pointer_first_param() {
    let f = make_ir_fn_with_param_types("bad", &[IrType::I32, IrType::I32, IrType::I32], &[]);
    assert!(validate_render_texture_sig_ir(&f).is_err());
}
```

**Cranelift JIT smoke (Q10 #5):**

A small handwritten LPIR module containing a no-op
`__render_texture_smoke(tex_ptr, w, h)` that just writes a constant
`0xFFFFu16` to `tex_ptr[0]`. Compile through `lpvm-cranelift`,
construct a small `LpvmBuffer`, call `call_render_texture`, assert
the byte at `tex_ptr[0..2]` is `[0xFF, 0xFF]` and `call_render_texture`
returns `Ok(())`. Lives in `lpvm-cranelift/src/tests/render_texture_smoke.rs`
(create file).

This test is the *only* per-backend runtime check in M2.0 — it
catches host-JIT regressions in the trait extension before lp-cli
demo breaks. The other five backends rely on `cargo build` here and
the format tests in Phase 5.

> The smoke test handwrites LPIR (not GLSL) so it's independent of
> Phase 3's synth layer. It lives in this phase.

## Validate

```bash
cargo check -p lpvm
cargo build --workspace --all-features    # catches every backend impl
cargo test  -p lpvm                       # validate_render_texture_sig tests
cargo test  -p lpvm-cranelift             # JIT smoke
```

The other backends compile here but get runtime coverage in Phase 5
(`lpvm-native`) or via integration tests outside M2.0 scope (Q10
analysis).
