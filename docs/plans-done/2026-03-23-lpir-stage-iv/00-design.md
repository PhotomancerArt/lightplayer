# Stage IV: Naga → LPIR Lowering — Design

## Scope

Implement the Naga → LPIR lowering pass in `lp-glsl-naga`. Covers scalar
expressions, control flow, user function calls, math builtin decomposition,
LPFX call structure, and interpreter-based testing. The lowering is completely
float-mode-unaware.

## File structure

```
lp-shader/lp-glsl-naga/
├── Cargo.toml                    # UPDATE: add lpir, lp-glsl-builtin-ids deps
└── src/
    ├── lib.rs                    # UPDATE: pub mod lower + submodules
    ├── lower.rs                  # NEW: entry point (lower fn, LowerError)
    ├── lower_ctx.rs              # NEW: LowerCtx (expr cache, param aliases, vreg alloc)
    ├── lower_expr.rs             # NEW: expression lowering (Naga Expression → LPIR ops)
    ├── lower_stmt.rs             # NEW: statement lowering (Naga Statement → LPIR ops)
    ├── lower_math.rs             # NEW: math builtin decomposition + std.math imports
    ├── lower_lpfx.rs             # NEW: LPFX detection, import creation, out-param ABI
    └── std_math_handler.rs       # NEW: StdMathHandler (ImportHandler for tests)

lp-shader/lp-glsl-naga/tests/
    ├── lower_interp.rs           # NEW: GLSL → LPIR → interpret → verify results
    └── lower_print.rs            # NEW: GLSL → LPIR → print → verify text output

lp-shader/lpir/src/
    └── op.rs                     # UPDATE: add Fabs, Fsqrt, Fmin, Fmax, Ffloor,
                                  #         Fceil, Ftrunc, Fnearest
```

## Conceptual architecture

```
GLSL source
  │
  ▼
compile(glsl) ──────────────────── lp-glsl-naga (existing)
  │
  ▼
NagaModule { module, functions }
  │
  ▼
lower(&NagaModule) ─────────────── lower.rs (new)
  │
  ├─ ModuleBuilder::new()
  ├─ collect LPFX imports ──────── lower_lpfx.rs
  ├─ collect std.math imports ──── lower_math.rs
  │
  ├─ for each user function:
  │   ├─ FunctionBuilder::new()
  │   ├─ add params (aliasing) ── lower_ctx.rs
  │   ├─ lower statements ─────── lower_stmt.rs
  │   │   ├─ Emit → no-op
  │   │   ├─ Block → recurse
  │   │   ├─ If → push_if / push_else / end_if
  │   │   ├─ Loop → push_loop / push_continuing / end_loop
  │   │   ├─ Break/Continue → Op::Break / Op::Continue
  │   │   ├─ Return → push_return
  │   │   ├─ Store → lower expr, assign to local VReg
  │   │   └─ Call → user call / LPFX call / math call
  │   └─ lower expressions ─────── lower_expr.rs
  │       ├─ cache: Vec<Option<VReg>> by Handle<Expression>
  │       ├─ Literal/Constant/ZeroValue → const ops
  │       ├─ FunctionArgument → param VReg
  │       ├─ Load(LocalVariable) → local VReg
  │       ├─ Binary → LPIR binary ops (kind-aware)
  │       ├─ Unary → LPIR unary ops
  │       ├─ Select → Op::Select
  │       ├─ As → cast ops (FtoiSatS/U, ItofS/U, etc.)
  │       ├─ CallResult → VReg from call results
  │       └─ Math → lower_math.rs
  │
  ▼
IrModule { imports, functions }
```

## Main components

### `lower.rs` — entry point

```rust
pub fn lower(naga: &NagaModule) -> Result<IrModule, LowerError>
```

Orchestrates the lowering. Builds the module-level import table (std.math +
lpfx), then lowers each user function. Returns `IrModule` or `LowerError`.

### `lower_ctx.rs` — per-function context

`LowerCtx` holds:

- `FunctionBuilder` — for emitting ops and allocating VRegs
- Expression cache: `Vec<Option<VReg>>` indexed by `Handle<Expression>`
- Parameter alias map: `Handle<LocalVariable>` → `VReg` (detected upfront)
- Local variable map: `Handle<LocalVariable>` → `VReg`
- Function index map: `Handle<Function>` → `CalleeRef`
- Reference to the Naga `Module` and `Function`

Expression caching: when an expression is first referenced as an operand, it
gets lowered and the result VReg is cached. Subsequent references reuse the
cached VReg. This handles Naga's DAG-shaped expression arena correctly.

Parameter aliasing: scan the function body for `Store(LocalVariable,
FunctionArgument)` patterns. When found, the local variable's VReg aliases
the parameter's VReg (no copy needed).

### `lower_expr.rs` — expression lowering

One function per expression variant. Resolves Naga `ScalarKind` to pick the
correct LPIR op (e.g. `Fadd` vs `Iadd`, `IltS` vs `IltU`).

Type mapping: `Float` → `F32`, `Sint`/`Uint`/`Bool` → `I32`.

### `lower_stmt.rs` — statement lowering

Walks Naga `Statement` trees. `Emit` ranges are no-ops (expressions lowered
on demand). Uses `FunctionBuilder`'s structured control flow helpers.

Loop lowering: when `continuing` is non-empty, calls `push_continuing()`.
When `break_if` is present, emits the condition + `BrIfNot` (negated) at
the end of the continuing section.

### `lower_math.rs` — math builtins

Three tiers:

**Tier 1 — New LPIR primitive ops** (both Cranelift and WASM have native):
`Fabs`, `Fsqrt`, `Fmin`, `Fmax`, `Ffloor`, `Fceil`, `Ftrunc`, `Fnearest`

**Tier 2 — Inline decomposition** (arithmetic sequences):

- `mix(x, y, t)` → `fsub(y, x)` → `fmul(_, t)` → `fadd(x, _)`
- `smoothstep(e0, e1, x)` → range, clamp via fmin/fmax, polynomial
- `step(edge, x)` → `fge(x, edge)` → `select(_, 1.0, 0.0)`
- `mod(x, y)` → `fdiv` → `ffloor` → `fmul` → `fsub`
- `fract(x)` → `ffloor(x)` → `fsub(x, _)`
- `clamp(x, lo, hi)` → `fmax(x, lo)` → `fmin(_, hi)`
- `sign(x)` → comparisons + select + constants
- Integer abs/min/max → comparison + select/negate

**Tier 3 — Import calls** (`@std.math::...`):
round, sin, cos, tan, asin, acos, atan, atan2, sinh, cosh, tanh, asinh,
acosh, atanh, exp, log, exp2, log2, pow, inversesqrt, fma, ldexp

### `lower_lpfx.rs` — LPFX handling

Detects `lpfx_*` calls via name + parameter type matching (reuses
`lp-glsl-builtin-ids` for resolution). Creates `@lpfx::name(...)` import
declarations. For out-parameters: allocates slots via `alloc_slot`, passes
slot address as i32 arg, loads results from slot after call.

### `std_math_handler.rs` — test support

Implements `lpir::ImportHandler` for the `std.math` module. Dispatches to
Rust's `f32` methods (sin, cos, round, etc.). Used by interpreter tests.

### New LPIR ops (in `lpir/src/op.rs`)

| Op         | Fields              | Semantics                        |
|------------|---------------------|----------------------------------|
| `Fabs`     | `{ dst, src }`      | `dst =                           |src|` |
| `Fsqrt`    | `{ dst, src }`      | `dst = sqrt(src)`                |
| `Fmin`     | `{ dst, lhs, rhs }` | `dst = min(lhs, rhs)`            |
| `Fmax`     | `{ dst, lhs, rhs }` | `dst = max(lhs, rhs)`            |
| `Ffloor`   | `{ dst, src }`      | `dst = floor(src)`               |
| `Fceil`    | `{ dst, src }`      | `dst = ceil(src)`                |
| `Ftrunc`   | `{ dst, src }`      | `dst = trunc(src)` (toward zero) |
| `Fnearest` | `{ dst, src }`      | `dst = roundEven(src)`           |

These are float-mode-agnostic. Emitters implement per-mode: float mode maps
to native instructions (WASM `f32.abs`, `f32.sqrt`, etc.); Q32 mode maps to
integer sequences or builtin calls.
