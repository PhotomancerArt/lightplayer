# Stage VI-B: fw-emu + lp-engine migration — notes

Plan name: **`lpir-cranelift-stage-vi-b`**. Roadmap:
`docs/roadmaps/2026-03-24-lpir-cranelift/stage-vi-b-fw-emu.md`.

**Process:** Resolve questions (one at a time) → **`00-design.md`** → phases → implement.

---

## Takeaways from Stage VI-A (keep in mind)

- **`lpir-cranelift` default `std`:** `jit()` and `lp-glsl-naga` are enabled by the
  default **`std`** feature. **`fw-emu` / `lp-server` / `lp-engine`** should keep
  **`std`** (or equivalent default features) so on-device GLSL → JIT works.
  **`cargo check --no-default-features`** on `lpir-cranelift` is IR-only; not the
  firmware path.
- **`LowMemory` vs emit order:** In **`module_lower`**, **`MemoryStrategy::LowMemory`**
  currently overrides **`LpirFuncEmitOrder::Name`** (object path). Engine JIT uses
  **`Source`** order only — **`LowMemory` + `Source`** is fine. Do not pass
  **`LowMemory`** into **`object_bytes_from_ir`** until that interaction is fixed
  or documented.
- **`Q32Options` / `max_errors`:** Types exist on **`CompileOptions`**; emitter may
  not yet branch on Q32 modes; **`max_errors`** may not be enforced. Engine should
  still **map** **`GlslOpts → lpir_cranelift::Q32Options`** and set
  **`max_errors`** for forward compatibility.
- **Docs:** Comment that GLSL-in “requires **`std`**” means **feature wiring**, not
  that **`lp-glsl-naga`** is **`std`**-only (it is **`#![no_std]`**).

---

## Scope of work

- Replace **`lp-glsl-cranelift`** with **`lpir-cranelift`** in **`lp-engine`** (and
  feature forwarding in **`lp-server`**).
- Rework **`ShaderRuntime`** (`runtime.rs`): compile via **`lpir_cranelift::jit`** (or
  orchestrated **`lp_glsl_naga` + `jit_from_ir_owned`** if we split for memory),
  store **`JitModule`** (or thin wrapper), implement **`lp_glsl_exec::GlslExecutable`**
  for the slow path (**`call_vec`** with **`lp_glsl_values::GlslValue`**).
- Replace **`get_direct_call_info`** (not on **`lp_glsl_exec::GlslExecutable`**) with
  **`JitModule::direct_call("main")`** → **`DirectCall`**, or equivalent.
- Map **`ShaderConfig::glsl_opts`** → **`CompileOptions`** (**`Q32Options`**,
  **`MemoryStrategy`** from **`memory_optimized`**, **`max_errors`**).
- Validate **`fw-emu`** build + integration tests; **`lp-engine`** tests; desktop
  host JIT.

**Out of scope:** **`fw-esp32`** (VI-C), delete old compiler (VII), full
**`Q32Options`** semantics in emitter if still deferred.

---

## Current codebase state

### `lp-engine` / `ShaderRuntime` (`nodes/shader/runtime.rs`)

- **`glsl_jit_streaming`** + **`GlslOptions`** (**`RunMode::HostJit`**, **`FloatMode::Q32`**,
  **`Q32Options`** from config, **`memory_optimized`**, **`max_errors`**).
- Stores **`Box<dyn GlslExecutable + Send + Sync>`** from old crate; trait is
  **`lp_glsl_cranelift::GlslExecutable`** (extends **`get_direct_call_info`**).
- **Fast path:** **`get_direct_call_info("main")`** → cached **`func_ptr`**,
  **`CallConv`**, **`Type`**; **`render_direct_call`** uses
  **`lp_glsl_jit_util::call_structreturn_with_args`** with Q32-packed args.
- **Slow path:** **`executable.call_vec`** with **`lp_glsl_cranelift::GlslValue`**.
- **`cranelift-codegen`** direct dep for **`CallConv`** / **`Type`** in struct.

### `lp-glsl-exec` (`executable.rs`)

- **`GlslExecutable`** without **`get_direct_call_info`** (by design for V2).

### `lp-glsl-filetests` (`lpir_jit_executable.rs`)

- **`LpirJitExecutable`**: wraps **`JitModule`**, implements **`lp_glsl_exec::GlslExecutable`**
  via **`Q32ShaderExecutable`** helpers — pattern to reuse or extract.

### `lpir-cranelift`

- **`jit(source, &CompileOptions) -> JitModule`**
- **`JitModule::direct_call`** → **`DirectCall`** with **`call_i32`** (**`invoke`**).
- **`CompilerError`** vs old **`GlslDiagnostics`**.

### `lp-server` / `fw-emu`

- **`lp-server`** forwards **`std`**, **`cranelift-optimizer`**, **`cranelift-verifier`**
  to **`lp-engine`** today via **`lp-glsl-cranelift`** paths — must switch to
  **`lpir-cranelift`** feature names.

---

## Questions

### Q1 — Where should the `JitModule` + `GlslExecutable` adapter live?

**Context:** **`ShaderRuntime`** needs **`dyn GlslExecutable + Send + Sync`**. Filetests
already have **`LpirJitExecutable`** in **`lp-glsl-filetests`**, but **`lp-engine`**
should not depend on filetests.

**Suggested answers:**

- **A)** **New small crate** (e.g. **`lp-glsl-lpir-exec`**) — **`JitModule`** wrapper,
  **`GlslExecutable`**, shared by **`lp-engine`** and optionally filetests later.
- **B)** **Private module inside `lp-engine`** (e.g. **`lpir_executable.rs`**) — copy
  or slim the filetest adapter; no new crate.
- **C)** **Optional feature on `lp-glsl-exec`** pulling **`lpir-cranelift`** — keeps
  one trait home; heavier coupling of exec crate to backend.

### Q2 — `glsl_jit_streaming` vs single-shot `jit()`

**Context:** Old path compiles AST per function (streaming). **`lpir-cranelift`**
 **`jit()`** parses/lowers the whole shader then builds one **`JitModule`**. Peak
 memory may differ; VI-A concluded batch **`finalize_definitions`** is fine.

**Suggested answers:**

- **A)** **VI-B uses `jit()` only**; measure / document peak vs old path; add
  streaming later if **`fw-emu`** OOMs.
- **B)** **Before VI-B**, add **`lpir`**-level streaming (larger scope).

### Q3 — Fast path: `DirectCall::call_i32` vs `call_structreturn_with_args`

**Context:** Today the engine uses **`lp_glsl_jit_util`** + **`cranelift-codegen`**
 types. **`DirectCall::call_i32`** should match the same struct-return layout as
 **`invoke`**.

**Suggested answers:**

- **A)** **Switch to `DirectCall::call_i32`** (or **`unsafe invoke`**) and pack/unpack
  **`i32`** like the typed path; **drop `cranelift-codegen`** from **`lp-engine`** if
  no other uses.
- **B)** **Keep `call_structreturn_with_args`** and **`cranelift-codegen`** for VI-B
  (minimal ABI risk); migrate ABI in a follow-up.

### Q4 — Compilation errors: `CompilerError` vs `GlslDiagnostics`

**Context:** **`compile_shader`** maps failures with **`format!("{e}")`**.

**Suggested answers:**

- **A)** **Keep `Display`**-only mapping in VI-B.
- **B)** **Map** subsets to **`GlslError`** where **`lp_glsl_diagnostics`** aligns
  (more work).

---

## Answers

### Q1 — Where should the `JitModule` + `GlslExecutable` adapter live?

**Answer: D) Store `JitModule` directly, no trait object.** `lp-engine` only ever
JIT-compiles — there's no polymorphism across backends. `JitModule` provides
`direct_call("main")` for the fast path and `call("main", &[GlslQ32])` for any
fallback. Drop `dyn GlslExecutable` and the `lp-glsl-exec` dependency from
`lp-engine`. Add `unsafe impl Send + Sync for JitModule` in `lpir-cranelift`
(finalized code pointers are stable and immutable after compilation;
`NodeRuntime: Send + Sync` requires it).

### Q2 — `glsl_jit_streaming` vs single-shot `jit()`

**Answer: A) Use `jit()` in VI-B.** Memory profile is comparable — both paths
batch-finalize, both drop intermediate IR per function. `fw-emu` has good
memory profiling tools; any peak regression will be caught there. LPIR-level
streaming is a follow-up if needed.

### Q3 — Fast path: `DirectCall::call_i32` vs `call_structreturn_with_args`

**Answer: Switch to `DirectCall`, drop `cranelift-codegen` from `lp-engine`.**
Add a non-allocating `call_i32_buf(&self, args: &[i32], out: &mut [i32])` to
`DirectCall` (and a matching `invoke_i32_buf` variant in `invoke.rs`) so the
engine's per-pixel hot loop writes into a stack `[i32; 4]` — zero heap
allocations. The existing `call_i32` (returning `Vec`) stays for convenience
elsewhere. Drop `cranelift-codegen` and `lp-glsl-jit-util` from `lp-engine`.

### Q4 — Compilation errors: `CompilerError` vs `GlslDiagnostics`

**Answer: A) `Display`-only mapping.** `CompilerError` → `format!("{e}")` →
`Error::InvalidConfig`. Engine never inspects error variants, just stringifies.
Same pattern as today with a different error type.

---

## Notes

- Roadmap: clean swap, no A/B feature flags in-tree.
- **`panic-recovery`:** preserve **`catch_unwind`** around compile; map panics to a
  stable error string as today.
