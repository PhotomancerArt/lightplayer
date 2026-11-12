# WASM Backend: Bump-Allocated Scratch Locals

## Problem

The WASM codegen pre-allocates fixed-size scratch pools for vector operations:

- `binary_op_f32_base` / `binary_op_i32_base`: 8 slots each
- `vector_conv_f32_base` / `vector_conv_i32_base`: 4 slots each

These are allocated once per function in `stmt/mod.rs` and shared by all
operations. The pool sizes were chosen for binary ops (4 lhs + 4 rhs = 8), but
three-argument builtins (`smoothstep`, `mix`, `clamp`) pack all args into the
same pool. For `smoothstep(vec3, vec3, vec3)`: 3×3 args + 2 temps = 11 > 8.

The limit is artificial — WASM locals are cheap (stack-allocated, no limit).

## Approach

Replace the fixed-size pool fields with a bump allocator on `WasmCodegenContext`.
Each emission site allocates exactly the locals it needs. No sharing, no aliasing,
no slot counting.

## What changes, what stays

**Replace with bump allocation** (high-level pools, used by operation entry points):

| Field | Slots | Used by |
|---|---|---|
| `binary_op_f32_base` | 8 × f32 | `binary.rs`, `builtin_inline.rs`, `builtin_call.rs`, `constructor.rs` |
| `binary_op_i32_base` | 8 × i32 | same |
| `vector_conv_f32_base` | 4 × f32 | `constructor.rs` (via `vector_conv_temp`) |

**Keep pre-allocated** (low-level primitives, called many times per function):

| Field | Slots | Reason |
|---|---|---|
| `vector_conv_i32_base` | 4 × i32 | Used by `emit_q32_add_sat` / `emit_q32_sub_sat` — called per Q32 ± op |
| `q32_mul_scratch` | 2 × i32 + 1 × i64 | Used by Q32 multiply — called per Q32 × op |
| `minmax_scratch_i32` | 2 × i32 | Used by `emit_i32_min_max_stack` — called per Q32 min/max |
| `broadcast_temp_f32` | 1 × f32 | Single temp used by multiple builtins |
| `broadcast_temp_i32` | 1 × i32 | Same |

Rationale: the kept pools are used by leaf-level helpers that are called
O(dim × operations) times per function. Bump-allocating per call would create
dozens of redundant locals. The replaced pools are used by entry-point-level
emission (one allocation per expression), where a few extra locals are harmless.

## Steps

### Step 1 — Add alloc methods to `WasmCodegenContext`

In `context.rs`, add:

```rust
pub fn alloc_f32(&mut self, count: u32) -> u32
pub fn alloc_i32(&mut self, count: u32) -> u32
pub fn alloc_i64(&mut self, count: u32) -> u32
```

Each bumps a cursor inside **pre-reserved scratch pools** appended to
`self.local_types` in `stmt/mod.rs` **before** `Function::new` (WASM requires
the function’s local count to be fixed at construction time; growing
`local_types` during emission would desync the encoder). Scratch pool sizes live in
`WASM_SCRATCH_*_POOL` in `stmt/mod.rs` (currently **1024×f32, 1024×i32, 32×i64** as a
temporary cap so per-call frames stay small; replace with exact high-water sizing
before raising limits). Exhaustion panics with a clear message.

Remove the `binary_op_temp_base` and `vector_conv_temp` methods (callers will
use the alloc methods directly).

### Step 2 — Migrate `builtin_inline.rs`

The biggest beneficiary. For each three-arg builtin (`emit_smoothstep`,
`emit_mix`, `emit_clamp`):

- Replace `ctx.binary_op_f32_base.ok_or(...)? + offset` with
  `ctx.alloc_f32(needed)` at the top of the function.
- Remove the `total > 8` / `temps_after + 2 > 8` guard checks.
- Same for the Q32 paths using `binary_op_i32_base`.

Two-arg builtins (`emit_vectorwise_binary_float`, `emit_mod`, `emit_sign`,
etc.) get the same treatment but are less urgent since they fit in 8.

Also fix the `local_tee` → `local_set` bug in the Float smoothstep loop
(line 1168). The `tee` leaves an extra value on the stack per component.

### Step 3 — Migrate `binary.rs`

`emit_vector_binary` currently does:
```rust
let base = ctx.binary_op_temp_base(&result_ty);
let (lhs_base, rhs_base) = (base, base + 4);
```

Replace with:
```rust
let lhs_base = ctx.alloc_X(dim);  // X = f32 or i32 depending on mode
let rhs_base = ctx.alloc_X(dim);
```

This also saves locals for small vectors — `vec2 + vec2` allocates 4 instead
of 8.

### Step 4 — Migrate `constructor.rs` and `builtin_call.rs`

`constructor.rs` uses `binary_op_i32_base` as a single temp in 4 places.
Replace with `ctx.alloc_i32(1)`.

`builtin_call.rs` uses `binary_op_i32_base` for storing builtin call args.
Replace with `ctx.alloc_i32(total_slots)`.

`vector_conv_temp` calls in constructors: replace with `ctx.alloc_f32(count)`
or `ctx.alloc_i32(count)` as appropriate.

### Step 5 — Remove dead fields and pre-allocation

In `context.rs`: remove `binary_op_f32_base`, `binary_op_i32_base`,
`vector_conv_f32_base` fields and the `binary_op_temp_base()` /
`vector_conv_temp()` methods.

In `stmt/mod.rs`: remove the corresponding `for _ in 0..8` /
`for _ in 0..4` allocation loops.

### Step 6 — Validate

- Run existing filetests (both Float and Q32 modes).
- The `common-smoothstep.glsl` vec3 test should now pass instead of erroring.
- Run the web demo with the rainbow shader.
- Linked `q32_builtin_link` rainbow row test: comparing two `fragCoord.x` values
  at `time = 1.0` can yield **identical black** pixels even when `prsd_demo`’s
  `tv` differs, because palette 0 (heatmap) maps many `tv.x` values to
  `vec3(0)`. Use a `time` on another palette (e.g. `6.0` → rainbow) or assert on
  a stage that exposes noise directly. `test_psrdnoise_output_differs_for_adjacent_pixels`
  guards LPFX + `fragCoord` wiring independently.

## Risks

**Local count inflation**: each operation gets unique locals instead of reusing
a shared pool. A function with 20 vector additions now gets 20×2×dim locals
instead of 8. For typical shaders this is hundreds of locals, not thousands.
WASM engines handle this fine — locals are a flat array in the stack frame.

**Nested aliasing**: `emit_q32_sub_sat` uses `vector_conv_i32_base` and is
called from inside per-component loops that now use bump-allocated scratch. Since
bump-allocated locals are always fresh indices, they never alias the kept
pre-allocated pools. This is strictly safer than the current design.
