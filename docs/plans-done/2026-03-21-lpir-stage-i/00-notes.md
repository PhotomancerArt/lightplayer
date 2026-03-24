# LPIR Stage I: Language Specification — Notes

## Scope

Define the complete LPIR language specification: operation set, type rules, text
format grammar, and semantics. The deliverable is a set of spec chapters in
`docs/lpir/` that serve as the reference for all subsequent implementation
(Stage II+).

No Rust code is written in this stage. The spec should be thorough enough that
Stage II implementation is mechanical.

## Current state

The existing WASM emitter (`lp-glsl-wasm/src/emit.rs`, ~1986 lines) handles
the following directly from Naga IR:

**Expressions**: Literal (F32/I32/U32/Bool), Constant, FunctionArgument,
CallResult, Load (LocalVariable), Binary (all arithmetic/comparison/logical/
bitwise), Unary (Negate/LogicalNot/BitwiseNot), Select, As (casts), ZeroValue,
Math (Mix/SmoothStep/Step/Round/Abs/Min/Max).

**Statements**: Emit, Block, If, Loop (with do-while splitting), Break,
Continue, Return, Store (LocalVariable), Call.

**Not handled**: Switch, Relational (any/all), dynamic array Access,
ArrayLength, F64/I64/AbstractFloat literals, Float modulo in Q32.

**Q32 fixed-point**: Q16.16 format. Add/Sub use i64 widen + op + saturate.
Mul uses i64 widen + mul + shr 16 + saturate. Div uses i64 widen + shl 16 +
div. Scratch locals for i64 saturation.

**LPFX builtins**: ~60 functions across generative (noise, fbm, worley),
color (hsv2rgb, rgb2hsv, hue2rgb), math (saturate), hash. Out-pointer ABI
for vector returns via linear memory at LPFX_SCRATCH_BASE (65536).

**Types**: Naga `ScalarKind { Float, Sint, Uint, Bool }`. WASM mapping:
Float→F32 (or I32 in Q32), Sint/Uint/Bool→I32.

## Questions

### 1. Op naming convention: type-prefixed vs unified enum?

The roadmap overview shows `float.add`, `i32.add`, `i64.add` (type-prefixed,
WASM-like). But LPIR is float-mode-agnostic, so `float.add` means "add two
float-kind values", not "f32 add". Meanwhile `i32.add` is concrete.

Two approaches:
- **(a) Type-prefixed**: `float.add`, `float.sub`, `i32.add`, `i32.sub`, etc.
  Each op encodes both the type and the operation. Clear, no ambiguity, matches
  WASM naming style.
- **(b) Unified + type from VReg**: `add v1, v2` where the type comes from the
  VReg's ScalarKind. Fewer ops, but type information is implicit.

**Suggested answer**: (a) Type-prefixed. The text format is more readable when
the type is visible at the operation site. VReg types still carry ScalarKind for
validation, but the op name makes intent clear. This matches the existing text
format examples in the overview.

**Answer**: Hybrid — **width-aware VReg types, short CLIF-style op names**.
VReg annotations carry the concrete width (`v0:f32`, `v1:i32`). Op names use
single-letter type prefix without width (`fadd`,
`isub`, `fconst`, `iconst`, `flt`, `ilt_s`, etc.). The op is unambiguous
because the result VReg type resolves the width. This keeps the text format
scannable while retaining full type information.

### 2. Integer width: i32-only or i32+i64?

The current codebase uses only i32 for Sint/Uint. i64 appears only as Q32
intermediate values (scratch locals). Naga supports i64 but GLSL 450 doesn't
have native 64-bit integers.

Options:
- **(a) i32-only in LPIR**: Sint/Uint VRegs are always 32-bit. i64 is an
  internal detail of the Q32 transform pass (which creates its own temporaries).
- **(b) i32+i64 in LPIR**: Include i64 ops. The Q32 transform emits i64 VRegs.

**Suggested answer**: (a) i32-only for the base spec.

**Answer**: **i32-only**. Q32 is now handled in each backend's emitter, not as
an LPIR→LPIR transform. Investigation of the Cranelift backend showed Q32
strategies are fundamentally backend-specific: WASM uses inline i64 sequences,
Cranelift saturating uses builtin calls (`__lp_q32_add`), Cranelift wrapping
uses all-i32 (`imul`+`smulhi`+shifts). A shared transform would pick one
representation that doesn't fit all backends. Consequence: LPIR has no i64
type. The type universe is f32 and i32 (GLSL `bool` lowers to i32 0/1).

### 3. Memory ops: scope and semantics?

The current emitter only uses memory for LPFX out-pointer ABI:
- `i32.store offset, value` — store to linear memory
- `i32.load offset` — load from linear memory
- Fixed scratch base address (65536)

Options:
- **(a) Minimal memory ops**: Just `store` and `load` with a type, offset, and
  base address. Enough for LPFX ABI. No general pointer arithmetic.
- **(b) More general memory model**: Add `ptr` type, pointer arithmetic, etc.

**Suggested answer**: (a) Minimal. We only need memory for LPFX import/export
ABI. The ops should reflect what's actually needed: store a scalar to a fixed
offset, load a scalar from a fixed offset.

**Answer**: **General pointer support via i32 addresses**. Pointers are just
i32 VRegs — no special pointer type. Use cases: LPFX out-pointer ABI,
out/inout parameters, local arrays, globals via context pointer.

Memory ops:
- `slot ssN, size` — function-level metadata declaring addressable memory
- `slot_addr ssN` → i32 — get base address of a slot (runtime op)
- `load base, offset` — load one scalar (type from result VReg)
- `store base, offset, value` — store one scalar (type from value VReg)
- `memcpy dst, src, size` — bulk copy for matrices/arrays

Pointer arithmetic uses regular `iadd`/`imul`. Slots map to Cranelift
`StackSlot`s and WASM shadow stack frames. `memcpy` maps to WASM
`memory.copy` and Cranelift `emit_small_memory_copy`/`call_memcpy`.

### 4. Call conventions: how to represent LPFX vs user calls?

Current emitter distinguishes:
- User function calls: direct call by function index
- LPFX builtin calls: imported functions with flattened i32 params, optional
  prepended result pointer, linear memory loads for vector results

Options:
- **(a) Single `call` op**: `call @name(args) -> result`. LPFX vs user is a
  property of the function declaration, not the call site.
- **(b) Separate `call` and `call_import`**: Different ops for local vs imported
  functions.

**Suggested answer**: (a) Single `call` op. The function declaration in the
module header indicates whether a function is local or imported. The call site
syntax is the same. The lowering handles flattening, pointer prepending, and
post-call memory loads — those expand to multiple LPIR ops (store, call, load).

**Answer**: **Single `call` op, differentiated function declarations**. The
call site syntax is always `call @name(args)`. Function declarations
distinguish import vs local:
- `import @__lp_q32_add(i32, i32) -> i32` — imported (Q32 builtin, LPFX)
- `func @my_helper(f32, f32) -> f32 { ... }` — local user function

The emitter uses the declaration to generate the right linkage (WASM import
vs internal function index, Cranelift ExternalName vs local FuncRef). The
lowering expands LPFX ABI (store args to slot, call, load results) into
explicit LPIR ops before the call.

### 5. ~~Do we spec the Q32 transform ops in this document?~~

**Resolved by Q2**: Q32 is handled per-backend in the emitter, not as an
LPIR→LPIR transform. No Q32-specific ops in the spec. The spec covers the
float-mode-agnostic op set (f32, i32). A brief note in the spec
should mention that backends handle Q32 internally and that float ops
(`fadd`, `fmul`, etc.) may be implemented as fixed-point arithmetic
depending on the target's FloatMode.

### 6. What math builtins belong in the op set vs are decomposed during lowering?

Current emitter has Math ops: Mix, SmoothStep, Step, Round, Abs, Min, Max.
Some of these could be decomposed during lowering (SmoothStep = clamp + hermite).
Others are close to hardware (Abs, Min, Max, Round).

Options:
- **(a) All as ops**: Keep Mix, SmoothStep, Step, Round, Abs, Min, Max as
  first-class LPIR ops. Backends map them to optimal sequences.
- **(b) Decompose complex ones**: Only keep Abs, Min, Max, Round as ops.
  Decompose Mix, SmoothStep, Step during lowering to arithmetic ops.
- **(c) Minimal**: Only keep what maps to single target instructions
  (Abs, Min, Max, Round). Everything else decomposes.

**Suggested answer**: (a) All as ops. Backends can emit optimal sequences
(e.g. WASM's `f32.min` for Min), and keeping them as named ops makes the IR
more readable. The lowering stays simple (Naga Math → LPIR Math, 1:1).
Decomposition can happen in an optimization pass later if desired.

**Answer**: **Separate `mathcall` mechanism, SPIR-V extended instruction
style**. Core Op enum stays small (arithmetic, comparison, logic, control
flow, memory). Math builtins use a `MathCall` op variant with a `MathFunc`
enum:

    v5:f32 = mathcall fmin(v3, v4)
    v6:f32 = mathcall fabs(v3)
    v7:f32 = mathcall fsmoothstep(v0, v1, v2)
    v8:f32 = mathcall fsin(v3)

In Rust: `Op::MathCall { dst, func: MathFunc, args }`. New builtins grow the
`MathFunc` enum without touching `Op`. The emitter maps `MathFunc` to optimal
sequences per backend. Mirrors SPIR-V's `OpExtInst` + `GLSL.std.450` and
Naga's `Expression::Math { fun: MathFunction }`.

### 7. How should the spec handle operations not yet needed (Switch, dynamic array access)?

Stage I says "design for the full scope even though scalar-only is implemented
first." Some features (Switch, dynamic Access, Relational) aren't used by any
current filetest but may be needed later.

Options:
- **(a) Include all known ops**: Spec them now, implement later.
- **(b) Core + reserved**: Spec what's needed now. Add a "reserved/future"
  section for known-needed-later ops.
- **(c) Core only**: Only spec what's immediately needed.

**Suggested answer**: (b) Core + reserved. Spec the full set needed for current
scalar filetests plus control flow/call patterns for Phase II. Add a reserved
section for Switch, dynamic array access, and Relational ops with notes on their
planned semantics. This avoids designing ourselves into a corner without
over-specifying unused features.

**Answer**: **(b) Core + reserved**. The spec fully defines ops needed for
current scalar filetests and Phase II control flow/call patterns. A "Future
Extensions" section lists known-needed ops (Switch, Relational ops like
any/all) with brief semantic notes. Dynamic array access is already covered
by the general pointer/load/store model.

## Notes

- The text format examples in the roadmap overview are already fairly concrete.
  The spec should formalize them into an actual grammar.
- The GLSL → LPIR mapping table should cover every Naga expression/statement
  variant we handle today, showing the corresponding LPIR op(s).
- VReg numbering: the overview uses `v0`, `v1`, etc. with monotonic allocation.
  The spec should define whether gaps are allowed, whether VRegs can be reused
  across functions, etc.
