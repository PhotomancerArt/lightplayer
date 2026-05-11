# Stage II: LPIR → CLIF Emitter Core — Notes

## Scope of work

Complete the `lpvm-cranelift` crate's core LPIR → Cranelift translation:
structured control flow via block stack, integer comparisons, local function
calls, and multi-return. No builtins, no Q32 mode, no imports — just the
structural translation layer, testable with `jit_from_ir` on hand-built LPIR
using native f32.

## Current state

### What already exists in `lpvm-cranelift`

The crate was scaffolded during Stage I work:

- **`Cargo.toml`** — dependencies on `lpir`, `cranelift-codegen`, `cranelift-frontend`,
  `cranelift-module`, `cranelift-jit`, `cranelift-native`, `target-lexicon`.
- **`emit.rs`** — translates a linear function body (no control flow):
    - All scalar float ops: `Fadd`, `Fsub`, `Fmul`, `Fdiv`, `Fneg`, `Fabs`,
      `Fsqrt`, `Fmin`, `Fmax`, `Ffloor`, `Fceil`, `Ftrunc`, `Fnearest`
    - All scalar integer arithmetic: `Iadd`, `Isub`, `Imul`, `IdivS`, `IdivU`,
      `IremS`, `IremU`, `Ineg`, `Iand`, `Ior`, `Ixor`, `Ibnot`, `Ishl`,
      `IshrS`, `IshrU`
    - Immediate ops: `IaddImm`, `IsubImm`, `ImulImm`, `IshlImm`, `IshrSImm`,
      `IshrUImm`, `IeqImm`
    - Constants: `FconstF32`, `IconstI32`
    - Float comparisons: `Feq`, `Fne`, `Flt`, `Fle`, `Fgt`, `Fge`
    - Casts: `FtoiSatS`, `FtoiSatU`, `ItofS`, `ItofU`
    - `Select`, `Copy`, `Return`
    - VReg → Cranelift Variable mapping, `def_var`/`use_var`
    - `signature_for_ir_func` helper
    - Catch-all `_ =>` returns `CompileError::Unsupported`
- **`jit_module.rs`** — `jit_from_ir(ir: &IrModule) -> Result<(JITModule, Vec<FuncId>)>`:
    - Rejects modules with imports
    - Creates host ISA via `cranelift_native::builder()`
    - Declares all functions, defines them, finalizes
    - Seals entry block immediately (no control flow)
- **`error.rs`** — `CompileError` with `Unsupported` and `Cranelift` variants
- **`lib.rs`** — exports + one test (`jit_linear_fadd_f32`)
- **Tests**: single test proving linear `fadd` compiles and runs

### What's missing for Stage II

1. **Structured control flow** — the main work:
    - `IfStart { cond, else_offset, end_offset }` → branch to then/else blocks,
      merge block
    - `Else` → switch from then-arm to else-arm
    - `End` → close current control structure, jump to merge/header
    - `LoopStart { continuing_offset, end_offset }` → header/exit blocks
    - `Break` → jump to loop exit
    - `Continue` → jump to loop header
    - `BrIfNot { cond }` → conditional branch to loop exit
    - Block stack to track nesting

2. **Integer comparisons** — small gap:
    - `Ieq`, `Ine`, `IltS`, `IleS`, `IgtS`, `IgeS` (signed)
    - `IltU`, `IleU`, `IgtU`, `IgeU` (unsigned)

3. **Local function calls** — `Op::Call` where `callee.0 >= import_count`:
    - Declare callee as Cranelift `FuncRef` in prologue
    - Emit `call` instruction with args from pool slice
    - Store results into destination VRegs

4. **Block sealing strategy** — current code seals entry block immediately.
   With control flow, blocks must be sealed after all predecessors are known
   (after branch instructions target them). Need to track which blocks to
   seal when.

5. **Tests** for control flow, local calls, multi-return, nested structures.

### LPIR control flow model

LPIR uses offset-annotated markers. Offsets point to other ops in the body
(Else, End, continuing point). The WASM emitter uses these to manage WASM's
structured control flow. For Cranelift, we need a different approach — Cranelift
uses a CFG (basic blocks + branches), not structured control flow.

The approach: maintain a **block stack** during the single-pass walk. Each
`IfStart`/`LoopStart` pushes control frame info (Cranelift blocks for
then/else/merge or header/exit). `End` pops and finalizes. `Break`/`Continue`
jump to the appropriate block from the stack.

Block sealing: Cranelift's `FunctionBuilder` needs blocks sealed after all
predecessors are known. For forward-only constructs (if/else), the merge block
can be sealed at `End`. For loops, the header block has a back-edge from
`Continue`/`End`, so it can't be sealed until the loop `End`.

### LPIR call model

`Op::Call { callee: CalleeRef, args: VRegRange, results: VRegRange }`. The
`CalleeRef(u32)` indexes imports first, then local functions. For Stage II
we only handle local calls (`callee.0 >= ir.imports.len()`). Import calls
are Stage III.

For local calls, we need `FuncRef`s. These come from declaring the callee
functions in the Cranelift module and importing them into the current function
via `module.declare_func_in_func(func_id, builder.func)`.

## Questions

### Q1: Should `translate_function` take additional context for FuncRefs?

Currently `translate_function(func, builder)` takes only the IrFunction and
FunctionBuilder. For local calls, it needs access to `FuncRef`s for other
functions. Options:

a) Pass a slice of `FuncRef` (one per callee in the module — imports + locals)
b) Pass a context struct with the FuncRef slice and module reference
c) Pass the full `&IrModule` + func refs

**Answer**: Context struct. Pass an `EmitCtx` holding `&[FuncRef]` (indexed by
local function index) and `&IrModule`. No borrowing issues — `FuncRef` is a
`Copy` index, created via `jit_module.declare_func_in_func(fn_id, builder.func)`
before `translate_function` is called. Stage III extends the struct with import
FuncRefs.

### Q2: Should we handle `SwitchStart`/`CaseStart`/`DefaultStart`?

The roadmap says "Not used yet by the Naga lowering, so not in scope." But
if the translation is simple, we could stub it.

**Answer**: Include it. In Cranelift's CFG model, switch is straightforward —
just blocks and branches (~30-40 lines). Easier than the WASM version which
fights structured control flow. Parity with the WASM emitter is worth having.

### Q3: Memory ops — include simple stack slots in this stage?

The roadmap's open question: "Slots, load, store are needed for out/inout
params and LPFX scratch. They could go in this stage or Stage III."

**Answer**: Include them. Stack slots, load, store, memcpy are core
infrastructure — Cranelift's native stack slots make them straightforward
(`create_sized_stack_slot`, `stack_addr`, `load`, `store`, `memory_copy`).
Remove the `uses_memory()` guard. Stage III only adds import resolution and
Q32 mode on top of a complete structural emitter.

### Q4: Block sealing strategy

Two options:
a) Seal blocks eagerly as soon as all predecessors are known (requires tracking)
b) Seal all blocks at the end via `builder.seal_all_blocks()`

**Answer**: (b) — `seal_all_blocks()` at the end of `translate_function`.
Simpler, standard for single-pass structured-CF translators. Negligible cost
at our function sizes.
