# Stage V1: RV32 object, linking, emulator — notes

## Scope of work

- Add an **RV32 `ObjectModule`** path in `lpvm-cranelift` that reuses the existing
  LPIR → CLIF emitter (`emit/`) with a RISC-V 32-bit ISA instead of host JIT.
- **Emit relocatable object** bytes (ELF) for the shader module.
- **Link** shader object into the pre-built **builtins emulator ELF** (same
  pattern as `lps-cranelift`: `lp-riscv-elf` + `lps-builtins-emu-app`
  bytes).
- **Run** linked code in **`lp-riscv-emu`** and validate with **in-crate tests**
  (hand-written or parsed LPIR, and optionally a thin GLSL → LPIR wrapper reusing
  `compile::jit`’s frontend half).
- **Feature-gate** RV32/object/emulator deps so default `lpvm-cranelift` stays
  host-JIT-oriented unless the feature is enabled.

**Out of scope:** `lps-filetests` targets `jit.q32` / `rv32.q32` (Stage V2).

## Current state of the codebase

### `lpvm-cranelift`

- Host-only: `cranelift-codegen` with `host-arch`, `JITModule`, `jit_from_ir`,
  `jit`, `JitModule`, `build_jit_module` in `jit_module.rs`.
- Lowering is centralized in `build_jit_module`: declare imports / opcode
  builtins, declare user funcs, `translate_function` per func,
  `define_function`, `finalize_definitions`.
- No `ObjectModule`, no `riscv32` ISA in this crate’s `Cargo.toml`.

### `lps-cranelift` (reference)

- `Target::riscv32_emulator()` → `isa_builder` + `riscv32` triple, flags via
  `default_riscv32_flags`.
- `ObjectBuilder::new(isa, b"module", default_libcall_names())` →
  `ObjectModule`.
- `backend/codegen/builtins_linker.rs`: `link_and_verify_builtins` loads
  builtins ELF, `load_object_file` merges shader object, verifies `BuiltinId`
  symbols.
- `backend/codegen/emu.rs`: defines all funcs in sorted name order, finishes
  object, links, runs emulator (`GlslEmulatorModule`).
- Builtins ELF: `build.rs` + `include!` generated `lp_builtins_lib.rs` (path to
  prebuilt `lps-builtins-emu-app`).

## Questions

### Q1: How much should we share with `lps-cranelift` vs duplicate?

**Context:** Linking and emulator orchestration already exist in the old crate;
`lpvm-cranelift` should stay the LPIR consumer and avoid pulling AST types.

**Suggested answer:** **Duplicate the small glue** (`link_and_verify_builtins`-style
logic, emulator options) inside `lpvm-cranelift` behind the feature flag, or
extract a tiny `lp-riscv-shader-link` crate later if duplication hurts. For V1,
prefer **local modules** (`object_link.rs`, `emu_run.rs`) that call
`lp-riscv-elf` / `lp-riscv-emu` directly, mirroring the old code paths.

### Q2: Refactor `jit_module.rs` vs parallel object builder?

**Context:** `build_jit_module` is ~200 lines of declare/define loop tied to
`JITModule`.

**Suggested answer:** **Extract a generic helper** `define_lpir_in_module<M:
Module>(module: &mut M, ir, …) -> Result<…>` that both JIT and object paths call,
so emit stays single-sourced. JIT-specific pieces: `JITBuilder`, symbol lookup for
JIT, `finalize_definitions` + `JitModule` wrapper. Object-specific: RV32 ISA,
`ObjectModule::finish`, raw bytes.

### Q3: Builtins ELF bytes — same build script as old crate?

**Context:** Emulator tests need the `lps-builtins-emu-app` artifact at
compile time.

**Suggested answer:** **Reuse the same mechanism:** `build.rs` in
`lpvm-cranelift` (feature-gated) that includes paths from env or known relative
path, documented in phase “Builtins linking”; same `scripts/build-builtins.sh`
workflow as today.

## Answers

### Q2: Refactor `jit_module.rs` vs parallel object builder?

**Answer:** **Generic refactor** — extract shared lowering as
`define_lpir_in_module` / `define_lpir_functions` over `M: Module`; JIT and RV32
object paths both call it. No duplicated declare/define loop for object-only.

## Notes

- Stage IV (`JitModule`, `call`, `direct_call`) remains the host path; V1 does
  not require `JitModule::call` on RV32 for minimal tests — raw emulator invoke
  or Q32-shaped `u32` buffers are enough for V1 smoke tests.
- Ordering relative to V2: V1 lands **before** filetests switch so
  `rv32.q32` never depends on two compilers.
