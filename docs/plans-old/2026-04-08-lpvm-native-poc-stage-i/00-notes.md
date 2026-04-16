# Plan notes: lpvm-native POC — stage i (M1 core + traits)

Roadmap: `docs/roadmaps/2026-04-07-lpvm-native-poc/m1-core-traits.md`  
Design: `docs/design/native/overview.md`

## Scope of work

- New crate `lp-shader/lpvm-native/`: `#![no_std]` + `alloc`, wired into workspace.
- `VInst` + lowering from LPIR (`Op`) for a **minimal** integer slice (roadmap: `iadd`, `isub`, `imul`; clarify vs builtins).
- `RegAlloc` trait + `GreedyAlloc` skeleton (interface + placeholder allocation if needed for tests).
- `IsaBackend` trait + RV32 module stub (no encoding).
- `NativeEngine` / `NativeModule` / `NativeInstance` implementing `lpvm` traits with `compile` / `instantiate` / `call` stubbed or returning clear errors — enough to `cargo check` and optional small unit tests on lowering.
- Register workspace member and any minimal dev-dependency for tests.

**Out of scope:** RV32 bytes, ELF, linking, real execution, linear scan, full opcode coverage.

## Current codebase state

- **`lpvm-native` crate**: does not exist; must be created from scratch.
- **`LpvmEngine` / `LpvmModule` / `LpvmInstance`** (`lp-shader/lpvm/`): associated types (`type Module`, `type Error`), not `dyn LpvmModule`. Mirror **`CraneliftEngine`** pattern: concrete `NativeModule`, `NativeInstance`, `NativeError`.
- **`CompileOptions`** today lives in **`lpvm-cranelift`** (`compile_options.rs`), depends on `lpir::FloatMode` and `lps_q32::Q32Options`. **`lps-shared` does not** define it — roadmap dependency list is slightly wrong.
- **`LpvmEngine::memory()`**: `CraneliftEngine` uses `CraneliftHostMemory`. For M1 stubs, **`lpvm::BumpLpvmMemory`** (or a tiny `NativeHostMemory` wrapper) can satisfy the trait without depending on Cranelift.
- **`lpir::IrType`**: already has `F32`, `I32`, `Pointer`. No `I64`. M1 “extended types” likely means a **backend-local** enum (e.g. `NativeType` or regalloc-facing types) that adds **`I64` stub**, or we document use of `I32` pair later — needs decision to avoid duplicating LPIR’s `IrType` confusingly.
- **Lowering `imul`**: LPIR `imul` is 32-bit integer multiply (maps to RV32 `mul` in full backend). Roadmap text “via builtins” may mean Q32 float path elsewhere; **integer `imul` should map to `VInst::Mul32`**, not `CallBuiltin`, unless we intentionally defer `imul` to M2.

## Questions (to resolve)

### Q1 — Where should `CompileOptions` (and `MemoryStrategy`) live for `NativeEngine`?

**Context:** `NativeEngine::new(options)` should align with Cranelift’s `FloatMode` / `Q32Options` so callers can swap backends without parallel option structs. Today options are defined only in `lpvm-cranelift`.

**Suggested answers:**

- **A (preferred long-term):** Move `CompileOptions` + `MemoryStrategy` to **`lpvm`** or **`lps-shared`**, update `lpvm-cranelift` to re-export or use them from there; `lpvm-native` depends on the same crate.
- **B (minimal M1):** `lpvm-native` defines **`NativeCompileOptions`** duplicating fields + `From`/`Into` or manual mapping; add a follow-up to unify.
- **C:** `lpvm-native` depends on **`lpvm-cranelift`** only to reuse `CompileOptions` — pulls Cranelift into the native backend’s dependency graph (bad for embedded story and compile time).

**Answer:** Each backend has its own options struct. `lpvm-wasm` has `WasmOptions { float_mode }`. `lpvm-cranelift` has `CompileOptions` with Cranelift-specific tuning. `lpvm-native` will define `NativeCompileOptions` (M1: just `FloatMode` or the fields needed). No shared options struct required; backends are intentionally not interchangeable at the type level (swapping is a code change, not a generic parameter).

---

### Q2 — Backend type system: extend `lpir::IrType` vs new `NativeType`?

**Context:** LPIR has no `I64`. Regalloc needs consistent typing for spill slot sizes and future 64-bit stubs.

**Suggested:** New **`NativeType`** (or `RegType`) in `types.rs`: `I32`, `I64` (stub), `F32`, `Ptr`, with `From<IrType>` for vregs coming from LPIR; do not modify `lpir::IrType` in M1.

**Answer:** **A** — `NativeType` in `lpvm-native/src/types.rs`, separate from `IrType`. LPIR types describe GLSL semantics; backend types describe register/spill representation. `From<IrType>` for the common case; backend can add `I64` stub, register class info, or spill slot size without affecting other backends. If we later need `I64` in LPIR itself, that's a cross-backend IR change, not a backend-local concern.

---

### Q3 — M1 lowering coverage: which `Op` variants must lower successfully in unit tests?

**Context:** Full `IrFunction` lowering is large; M1 should prove the pipeline for a **small** subset.

**Suggested:** Single test function or synthetic `IrFunction` builder: `iadd`, `isub`, `imul`, `iconst`, `return` only; **`lower_function` returns `Result`** and `UnsupportedOp` for everything else.

**Answer:** Include **calling** in M1 architecture (VInst, regalloc clobber tracking, lowering to `Call` VInst for Q32 float ops), but **stub emission** (panic/"M2: emit jal"). This validates the architecture handles ABI/register pressure without requiring full relocation/linking support. Integer ALU (`iadd`, `isub`, `imul`, `iconst`, `return`) + `Call` VInst is the M1 lowering coverage. Cranelift emission uses standard RISC-V psABI; we match it for consistency.

---

### Q4 — Workspace membership and ESP32 check

**Context:** AGENTS.md requires `cargo check -p fw-esp32 ...` for shader pipeline touches.

**Suggested:** Add `lp-shader/lpvm-native` to workspace `members` (and `default-members` if policy matches other shader crates). After crate exists, run **`cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server`** only if `fw-esp32` or `lp-engine` gains a dependency on `lpvm-native` in M1; if M1 is crate-only with no upstream edge, **`cargo check -p lpvm-native`** + **`cargo test -p lpvm-native --lib`** may suffice until integration lands.

**Answer:** **B** — add to `members` and `default-members` for visibility. M1 validation is `cargo check -p lpvm-native` + `cargo test -p lpvm-native --lib`. ESP32 check (`cargo check -p fw-esp32 --target...`) runs only when `lp-engine` or `fw-esp32` gain dependency on `lpvm-native` (post-M1).

## Notes

_(User answers and follow-ups go here.)_
