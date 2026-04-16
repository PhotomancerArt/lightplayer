# M7 Cleanup: Analysis and Questions

## Scope of Work

Final cleanup milestone for the LPVM2 project. Focus is on deleting obsolete code,
removing dead traits and crates, and verifying everything builds cleanly across
all targets.

### Key Cleanup Tasks

1. **Delete legacy crates and traits:**
   - `lp-shader/legacy/lps-exec/` (GlslExecutable trait)
   - `lp-shader/legacy/lps-wasm/` (old WASM emitter, replaced by lpvm-wasm)
   - `lp-shader/legacy/lps-builtins-wasm/` (if not used)

2. **Remove old `JitModule`/`jit()` API from `lpvm-cranelift`:**
   - Now replaced by `CraneliftEngine`/`CraneliftModule` via `LpvmEngine` trait
   - Only used in `lp-engine/src/gfx/cranelift.rs` via `jit()` function

3. **Clean up `lps-filetests`:**
   - `wasm_link.rs` - still required by `tests/lpfx_builtins_memory.rs`
   - Migrate that test to use `lpvm-wasm` instantiate/link path instead

4. **Update documentation:**
   - AGENTS.md architecture diagram references old pipeline
   - Update to reflect new LPVM trait-based architecture

5. **Validation:**
   - All targets compile with no warnings
   - All tests pass
   - Filetest matrix passes

## Current State

### Legacy Crates

```
lp-shader/legacy/
├── lps-exec/           # GlslExecutable trait - TO DELETE
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       └── executable.rs
├── lps-wasm/           # Old WASM emitter - TO DELETE
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       └── emit/
└── lps-builtins-wasm/  # Old builtins WASM - TO DELETE
    ├── build.rs
    ├── Cargo.toml
    └── src/
        └── lib.rs
```

### Current API Usage

- `lp-engine/src/gfx/cranelift.rs` uses `lpvm_cranelift::jit()` and `JitModule`
- `lp-shader/lps-filetests/src/test_run/wasm_link.rs` used by `tests/lpfx_builtins_memory.rs`
- `lp-shader/lpvm-emu/src/emu_run.rs` has `glsl_q32_call_emulated` vs `EmuInstance::call` paths

### Architecture Changes Needed

Old:
```
GLSL → lps-frontend → LPIR → lpvm-cranelift → JitModule → call()
```

New:
```
GLSL → lps-frontend → LPIR → CraneliftEngine (LpvmEngine) → CraneliftModule → DirectCall
```

## Questions

### Q1: Should we migrate `lp-engine` to use `CraneliftEngine` instead of `jit()` before deleting `jit()`?

**Context:** `lp-engine/src/gfx/cranelift.rs` currently imports and uses `lpvm_cranelift::jit` and `JitModule`. The new LPVM traits (`CraneliftEngine`/`CraneliftModule` implementing `LpvmEngine`/`LpvmModule`) exist and are used by `lps-filetests`. 

**Options:**
- A) Migrate `lp-engine` to use `CraneliftEngine` before deleting `jit()`
- B) Keep `jit()` as a simplified wrapper around `CraneliftEngine` for ergonomic use
- C) Delete `jit()` and update `lp-engine` in the same commit

**Answer:** Yes - this was supposed to be part of M6 but was missed. We must migrate `lp-engine` to use `CraneliftEngine` instead of `jit()` before deleting the old API. The trait-based abstraction should be fully wired end-to-end.

**Decision:** Migrate `lp-engine/src/gfx/cranelift.rs` to use `CraneliftEngine::compile()` → `CraneliftModule` → `direct_call()` instead of `jit()` → `JitModule::direct_call()`.

### Q2: What to do with `wasm_link.rs` in `lps-filetests`?

**Context:** `lps-filetests/src/test_run/wasm_link.rs` provides wasmtime linking for builtins + shader WASM. It's used by `tests/lpfx_builtins_memory.rs`. The new `lpvm-wasm` crate has its own instantiate/link path that could replace this.

**Options:**
- A) Migrate `lpfx_builtins_memory.rs` to use `lpvm-wasm` path, then delete `wasm_link.rs`
- B) Keep `wasm_link.rs` as a filetest-specific helper (rename to something clearer)
- C) Extract shared wasmtime helper into `lpvm-wasm` and use from both places

**Suggested:** A - The filetest should use the same code path as production. Migrate to `lpvm-wasm` and remove duplication.

### Q3: Should we delete all legacy crates at once or incrementally?

**Context:** Three legacy crates in `lp-shader/legacy/`: `lps-exec`, `lps-wasm`, `lps-builtins-wasm`. They may have inter-dependencies.

**Options:**
- A) Delete all at once in single phase
- B) Delete one per phase with validation between
- C) Keep `lps-exec` temporarily if diagnostics references it

**Suggested:** B - One per phase to keep diffs reviewable. Check workspace references between each deletion.

### Q4: AGENTS.md architecture diagram - full rewrite or incremental update?

**Context:** Current diagram shows old pipeline. New architecture uses `LpvmEngine`/`LpvmModule`/`LpvmInstance` traits with multiple backends.

**Options:**
- A) Full rewrite with new trait-based diagram showing Cranelift/WASM/Emu backends
- B) Minimal update mentioning LPVM traits and linking to detailed docs
- C) Keep simple pipeline view but update crate names

**Suggested:** A - The architecture has fundamentally changed. Show the trait-based abstraction clearly.

### Q5: Should we run full filetest matrix in CI-style validation?

**Context:** M5 cleanup included full filetest matrix validation. Should M7 repeat this to ensure no regressions during cleanup?

**Options:**
- A) Full matrix: `jit.q32`, `jit.f32`, `rv32.q32`, `rv32.f32`, `wasm.q32`, `wasm.f32`
- B) Representative subset (one per backend)
- C) Just `cargo test` and assume filetests are covered

**Suggested:** A - Cleanup shouldn't break anything. Full matrix ensures confidence.

### Q6: How to handle `emu_run.rs` in `lpvm-emu`?

**Context:** `lpvm-emu/src/emu_run.rs` provides `glsl_q32_call_emulated` and other helpers. `EmuInstance` now implements `LpvmInstance::call`/`call_q32`. The roadmap suggests consolidating these.

**Options:**
- A) Migrate all callers to `EmuInstance`, delete `emu_run.rs`
- B) Make `emu_run.rs` use `EmuInstance` internally (layering)
- C) Keep both - `emu_run` for simple cases, `EmuInstance` for trait-based

**Suggested:** A - Single path reduces maintenance burden and ABI drift risk.
