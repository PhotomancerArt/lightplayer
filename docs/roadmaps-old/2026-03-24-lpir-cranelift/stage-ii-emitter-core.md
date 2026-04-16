# Stage II: LPIR → CLIF Emitter Core

## Goal

Create the new `lpvm-cranelift` crate with the core LPIR → Cranelift
translation: scalar ops, structured control flow → CFG, VReg → Variable
mapping. No builtins, no Q32 mode, no imports — just the structural
translation layer. Testable with `jit_from_ir` on hand-built LPIR using
native f32.

## Suggested plan name

`lpvm-cranelift-stage-ii`

## Scope

**In scope:**

- New crate `lp-shader/legacy/lpvm-cranelift/` with Cargo.toml, `no_std` + `alloc`
  support, cranelift dependencies (cranelift-codegen, cranelift-frontend,
  cranelift-module, cranelift-jit)
- `emit.rs` — the core translation:
    - Block stack for structured CF → Cranelift blocks
    - `IfStart`/`Else`/`End` → then/else/merge blocks with branches
    - `LoopStart`/`End` → header/exit blocks
    - `Break`/`Continue` → jumps to exit/header
    - `BrIfNot` → conditional branch
    - `Return` → Cranelift return instruction
    - VReg → Cranelift `Variable`, `def_var`/`use_var`
    - Scalar arithmetic ops → Cranelift equivalents (`fadd`, `isub`, etc.)
    - Comparison ops → Cranelift `icmp`/`fcmp`
    - Constants → `iconst`/`f32const`
    - `Select` → Cranelift `select`
    - `Copy` → `def_var` alias
    - Cast ops → Cranelift conversion instructions
- `module.rs` — minimal JitModule wrapper:
    - Create Cranelift JITModule
    - Declare + define functions
    - Finalize
    - Basic function calling (for tests)
- `lib.rs` — `jit_from_ir(ir: &IrModule, ...) -> Result<JitModule>`
- `error.rs` — CompileError
- Tests: hand-built IrModule with arithmetic, control flow, multi-return
  → JIT compile → call → verify results

**Out of scope:**

- Q32 mode (Stage III)
- Builtin calls / imports (Stage III)
- Memory ops: slot, load, store, memcpy (Stage III)
- `jit()` from GLSL source (Stage IV)
- Level 1 typed call interface (Stage IV)
- GlslMetadata (Stage IV)
- Filetest integration (Stage V2)

## Key decisions

- The emitter walks `Vec<Op>` in a single pass, maintaining a block stack.
  No pre-analysis or CFG construction.
- Each LPIR `VReg` maps to exactly one Cranelift `Variable`. Variable
  declarations happen up front based on `IrFunction.vreg_types`.
- Function parameters: first N Variables are params, declared from
  Cranelift block params on the entry block.
- Multi-return: use Cranelift's native multi-return. If the target ABI
  requires struct-return, Cranelift handles it automatically.
- For this stage, all float ops emit native Cranelift `fadd`/`fmul`/etc.
  (f32 mode). Q32 remapping comes in Stage III.

## Open questions

- **Memory ops deferral**: Slots, load, store are needed for out/inout
  params and LPFX scratch. They could go in this stage or Stage III. I've
  placed them in Stage III since they're closely tied to the calling
  convention for builtins. But if simple stack slot ops are easy to add
  here, it may be cleaner to include them.
- **Switch statement**: LPIR has `SwitchStart`/`CaseStart`/`DefaultStart`.
  Not used yet by the Naga lowering, so not in scope. But if the
  translation is simple, might be worth stubbing.
- **Cranelift features**: Which cranelift-codegen features to enable?
  The old crate uses `riscv32` for the emulator path. For host JIT we
  need `host-arch` (std) or nothing (embedded defaults to riscv32).
  Mirror the old crate's feature structure or simplify?
- **`no_std` from day one?**: The old crate supported `no_std` with a
  `core` feature. Should the new crate start `no_std` or start with `std`
  and gate later? Starting `no_std` is cleaner but adds friction during
  development (no `println!` debugging, `alloc` everywhere).

## Deliverables

- `lp-shader/legacy/lpvm-cranelift/` crate
- Core emitter handling all scalar LPIR ops and structured CF
- Basic JitModule with function calling
- Tests proving arithmetic, conditionals, loops, multi-return work

## Dependencies

- Stage I (builtin naming) should be complete so the crate is built on
  the new naming from the start.

## Estimated scope

~600–800 lines of emitter + module + ~200 lines of tests.
