# Stage VI-B: fw-emu + lp-engine migration — design

## Scope

Replace `lp-glsl-cranelift` with `lpir-cranelift` in `lp-engine`. Rewrite
`ShaderRuntime` to compile via `lpir_cranelift::jit()`, store `JitModule`
directly (no trait object), and render via `DirectCall::call_i32_buf`
(zero-alloc per pixel). Drop `cranelift-codegen` and `lp-glsl-jit-util` from
`lp-engine`. Validate via `fw-emu` and desktop tests.

## File structure

```
lp-shader/lpir-cranelift/src/
├── direct_call.rs                # UPDATE: add call_i32_buf (non-allocating)
├── invoke.rs                     # UPDATE: add invoke_i32_buf variant
├── jit_module.rs                 # UPDATE: unsafe impl Send + Sync for JitModule
└── ...                           # rest unchanged

lp-core/lp-engine/
├── Cargo.toml                    # UPDATE: replace lp-glsl-cranelift → lpir-cranelift;
│                                 #   drop cranelift-codegen, lp-glsl-jit-util
└── src/nodes/shader/
    └── runtime.rs                # UPDATE: rewrite compile + render paths

lp-core/lp-server/
└── Cargo.toml                    # UPDATE: forward lpir-cranelift features

lp-fw/fw-emu/
└── Cargo.toml                    # CHECK: transitive deps, may need no changes
```

## Architecture

```
ShaderRuntime
  ├── module: Option<JitModule>           (replaces Box<dyn GlslExecutable>)
  ├── direct_call: Option<DirectCall>     (replaces func_ptr + CallConv + Type)
  │
  ├── compile_shader(source)
  │     └── lpir_cranelift::jit(source, &CompileOptions)
  │           ├── float_mode: Q32
  │           ├── q32_options: mapped from config.glsl_opts
  │           ├── memory_strategy: LowMemory or Default
  │           └── max_errors: Some(N)
  │     → JitModule
  │     → module.direct_call("main") → DirectCall
  │
  ├── render()                            fast path (always)
  │     └── direct_call.call_i32_buf(&args, &mut [i32; 4])
  │           zero-alloc per pixel, Q32 i32 in/out
  │
  └── (slow call_vec path removed; JitModule::call available if ever needed)
```

## Main components

### `DirectCall::call_i32_buf` (new)

Non-allocating variant of `call_i32`. Writes return values into a caller-
provided `&mut [i32]` buffer. Internally dispatches to the same `invoke`
machinery (CRet structs on non-AArch64, inline asm on AArch64) but copies
results to the output slice instead of building a `Vec`.

```rust
impl DirectCall {
    pub unsafe fn call_i32_buf(
        &self,
        args: &[i32],
        out: &mut [i32],
    ) -> Result<(), CallError> { ... }
}
```

### `JitModule` Send + Sync

```rust
// Finalized JIT code pointers are stable and immutable after compilation.
// JITModule's interior RefCell is only used during construction (define/finalize),
// never after build_jit_module returns.
unsafe impl Send for JitModule {}
unsafe impl Sync for JitModule {}
```

### `ShaderRuntime` rewrite

**Fields replaced:**

| Old                                                         | New                               |
|-------------------------------------------------------------|-----------------------------------|
| `executable: Option<Box<dyn GlslExecutable + Send + Sync>>` | `module: Option<JitModule>`       |
| `direct_func_ptr: Option<FunctionPtr>`                      | `direct_call: Option<DirectCall>` |
| `direct_call_conv: Option<CallConv>`                        | _(folded into DirectCall)_        |
| `direct_pointer_type: Option<Type>`                         | _(folded into DirectCall)_        |

**`compile_shader`:**

```rust
use lpir_cranelift::{jit, CompileOptions, MemoryStrategy, Q32Options, FloatMode};

let q32_options = Q32Options {
    add_sub: map_add_sub(config.glsl_opts.add_sub),
    mul: map_mul(config.glsl_opts.mul),
    div: map_div(config.glsl_opts.div),
};

let options = CompileOptions {
    float_mode: FloatMode::Q32,
    q32_options,
    memory_strategy: MemoryStrategy::LowMemory,  // embedded default
    max_errors: Some(16),
};

let module = jit(glsl_source, &options)?;
let direct_call = module.direct_call("main");
self.module = Some(module);
self.direct_call = direct_call;
```

**`render`:**

```rust
let dc = self.direct_call.as_ref().ok_or(...)?;
let mut result = [0i32; 4];
let args = [frag_x_q32, frag_y_q32, size_x_q32, size_y_q32, time_q32];
unsafe { dc.call_i32_buf(&args, &mut result)?; }
// result[0..4] = r, g, b, a in Q32
```

### `GlslOpts → Q32Options` mapping

Small helper functions in `runtime.rs` (or a private module):

```rust
fn map_add_sub(m: lp_model::glsl_opts::AddSubMode) -> lpir_cranelift::AddSubMode { ... }
fn map_mul(m: lp_model::glsl_opts::MulMode) -> lpir_cranelift::MulMode { ... }
fn map_div(m: lp_model::glsl_opts::DivMode) -> lpir_cranelift::DivMode { ... }
```

### Cargo dependency changes

**`lp-engine/Cargo.toml`:**

```toml
# Remove:
lp-glsl-cranelift = { ... }
cranelift-codegen = { ... }
lp-glsl-jit-util = { ... }

# Add:
lpir-cranelift = { path = "../../lp-shader/lpir-cranelift", default-features = false }
```

Features:

```toml
[features]
default = ["std", "cranelift-optimizer", "cranelift-verifier"]
std = ["lp-shared/std", "lpir-cranelift/std"]
cranelift-optimizer = ["lpir-cranelift/cranelift-optimizer"]
cranelift-verifier = ["lpir-cranelift/cranelift-verifier"]
```

**`lp-server/Cargo.toml`:** Same forwarding pattern, replacing
`lp-engine/cranelift-*` with the same feature names (already aligned).

### Error handling

`CompilerError` → `format!("{e}")` → `Error::InvalidConfig`. Same pattern as
today. `panic-recovery` wrapping preserved around `jit()` call.

## Validation

```bash
cargo test -p lp-engine
cargo test -p lpir-cranelift
cargo test -p lpir-cranelift --features riscv32-emu
cargo check --target riscv32imac-unknown-none-elf -p lpir-cranelift --no-default-features
# fw-emu build + run (scripts/build-fw-emu.sh or equivalent)
```
