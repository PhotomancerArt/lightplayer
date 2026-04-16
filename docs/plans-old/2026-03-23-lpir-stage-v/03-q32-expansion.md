# Phase 3: Q32 Float Expansion

## Scope

Implement `q32.rs` — all Q16.16 fixed-point expansion for LPIR float
ops. Replace the `todo!()` stubs from Phase 2. After this phase, all
arithmetic ops emit valid Q32 WASM.

## Implementation

### `emit/q32.rs`

Helper functions called from `ops.rs` when the op is a float operation.

### Constants

```rust
const Q16_16_SCALE: f32 = 65536.0;
const Q32_ONE: i32 = 65536;       // 1.0 in Q16.16
const Q32_MIN: i64 = i32::MIN as i64;
const Q32_MAX: i64 = i32::MAX as i64;
```

### `FconstF32` — literal conversion

```rust
fn emit_q32_fconst(value: f32) -> i32 {
    let clamped = value.clamp(-32768.0, 32767.99998);
    (clamped * Q16_16_SCALE) as i32
}
```

Emit: `i32.const <result>`

### Saturating arithmetic (Fadd, Fsub)

Both use the same pattern: widen to i64, operate, saturate back.

```
local.get lhs
i64.extend_i32_s
local.get rhs
i64.extend_i32_s
i64.add              // or i64.sub
local.tee $i64_scratch
i64.const Q32_MIN
i64.lt_s
if (result i32)
    i32.const i32::MIN
else
    local.get $i64_scratch
    i64.const Q32_MAX
    i64.gt_s
    if (result i32)
        i32.const i32::MAX
    else
        local.get $i64_scratch
        i32.wrap_i64
    end
end
local.set dst
```

Extract a shared `emit_q32_sat_from_i64` helper for the saturation
clamp pattern.

### Fmul

```
local.get lhs
i64.extend_i32_s
local.get rhs
i64.extend_i32_s
i64.mul
i64.const 16
i64.shr_s            // shift right by 16 to restore Q16.16 scale
→ emit_q32_sat_from_i64
local.set dst
```

### Fdiv

```
local.get lhs
i64.extend_i32_s
i64.const 16
i64.shl              // shift left numerator by 16
local.get rhs
i64.extend_i32_s
i64.div_s
i32.wrap_i64         // no saturation needed (result fits)
local.set dst
```

### Fneg

```
i32.const 0
local.get src
i32.sub
local.set dst
```

### Float comparisons (Feq, Fne, Flt, Fle, Fgt, Fge)

Q16.16 values are ordered as signed i32, so comparisons map directly:

| LPIR | WASM |
|------|------|
| `Feq` | `i32.eq` |
| `Fne` | `i32.ne` |
| `Flt` | `i32.lt_s` |
| `Fle` | `i32.le_s` |
| `Fgt` | `i32.gt_s` |
| `Fge` | `i32.ge_s` |

### Tier 1 math ops

**`Fabs`** — inline:
```
local.get src
local.get src
i32.const 0
i32.lt_s
if (result i32)
    i32.const 0
    local.get src
    i32.sub
else
    local.get src
end
local.set dst
```

**`Fmin`** — inline:
```
local.get lhs
local.get rhs
local.get lhs
local.get rhs
i32.lt_s
select
local.set dst
```

**`Fmax`** — inline:
```
local.get lhs
local.get rhs
local.get lhs
local.get rhs
i32.gt_s
select
local.set dst
```

**`Ffloor`** — inline (mask off lower 16 bits, adjust for negative):
```
local.get src
i32.const 0xFFFF0000    // -65536
i32.and
local.set dst
// If src was negative and had fractional bits, floor is one step lower.
// Actually: for Q16.16, floor = src & 0xFFFF0000 works for positive.
// For negative with nonzero fractional: floor = (src & 0xFFFF0000) - 0x10000
// But simpler: floor = (src >> 16) << 16 (arithmetic shift preserves sign)
```

Actually, `i32.and` with `0xFFFF0000` is equivalent to truncation toward
negative infinity only for positive numbers. For negative numbers with
fractional bits, it truncates toward zero, which is wrong for floor.

Correct Q32 floor:
```
local.get src
i32.const 16
i32.shr_s          // arithmetic shift right by 16 → integer part
i32.const 16
i32.shl            // shift back → floor in Q16.16
```
Wait — `shr_s 16` then `shl 16` zeros out fractional bits but preserves
the sign via arithmetic shift. For negative values: -1.5 (0xFFFE8000)
→ shr_s 16 → -2 (0xFFFFFFFE) → shl 16 → -2.0 (0xFFFE0000). Correct.

For positive: 1.5 (0x00018000) → shr_s 16 → 1 → shl 16 → 1.0. Correct.

**`Fceil`** — inline:
```
// ceil(x) = -floor(-x)
i32.const 0
local.get src
i32.sub              // -src
→ emit_q32_floor
i32.const 0
<result>
i32.sub              // negate
local.set dst
```

Or: `ceil(x) = floor(x + 0xFFFF)` (add just under 1.0, then floor).
```
local.get src
i32.const 0xFFFF     // (1.0 - epsilon) in Q16.16
i32.add
i32.const 16
i32.shr_s
i32.const 16
i32.shl
local.set dst
```

**`Ftrunc`** — toward zero: floor for positive, ceil for negative.
Simplest: mask off fractional bits:
```
local.get src
i32.const 0xFFFF
i32.and              // fractional part
// if src >= 0: trunc = src & 0xFFFF0000
// if src < 0 and frac != 0: trunc = (src & 0xFFFF0000) + 0x10000
```

Or use `src / 65536 * 65536` via shifts:
```
local.get src
i32.const 16
i32.shr_s
i32.const 16
i32.shl
local.set tmp        // this is floor(src)
// if src < 0 and tmp != src: trunc = tmp + 0x10000
local.get src
i32.const 0
i32.lt_s
local.get tmp
local.get src
i32.ne
i32.and              // src < 0 && floor != src
if
    local.get tmp
    i32.const 0x10000
    i32.add
    local.set dst
else
    local.get tmp
    local.set dst
end
```

**`Fsqrt`**, **`Fnearest`** — call builtins:
- `Fsqrt` → `builtins::__lp_q32_sqrt`
- `Fnearest` → `builtins::__lp_q32_roundeven`

### Casts (Q32)

**`FtoiSatS`** — Q32 fixed → signed int:
```
local.get src
i32.const 16
i32.shr_s            // Q16.16 → integer (truncate toward -inf)
local.set dst
```

**`FtoiSatU`** — Q32 fixed → unsigned int:
```
local.get src
i32.const 16
i32.shr_u
local.set dst
```

**`ItofS`** — signed int → Q32 fixed:
```
local.get src
i64.extend_i32_s
i64.const 16
i64.shl
→ emit_q32_sat_from_i64
local.set dst
```

**`ItofU`** — unsigned int → Q32 fixed:
```
local.get src
i64.extend_i32_u
i64.const 16
i64.shl
→ emit_q32_sat_from_i64
local.set dst
```

## Validate

```
cargo check -p lps-wasm
```

All arithmetic ops (integer and float Q32) now emit valid WASM.
