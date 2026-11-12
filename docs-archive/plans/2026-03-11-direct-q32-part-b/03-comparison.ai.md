# Phase 3: Comparison Operations

Implement `Q32Strategy` methods for cmp, min, max.

## Source

`backend/transform/q32/converters/comparison.rs`

## emit_cmp

The caller passes a `FloatCC` condition code. The strategy translates
to the equivalent `IntCC` and emits `icmp`:

```
FloatCC::Equal              → IntCC::Equal
FloatCC::NotEqual           → IntCC::NotEqual
FloatCC::LessThan           → IntCC::SignedLessThan
FloatCC::LessThanOrEqual    → IntCC::SignedLessThanOrEqual
FloatCC::GreaterThan        → IntCC::SignedGreaterThan
FloatCC::GreaterThanOrEqual → IntCC::SignedGreaterThanOrEqual
FloatCC::Ordered            → IntCC::Equal  (no NaN in fixed-point)
FloatCC::Unordered          → IntCC::NotEqual
FloatCC::OrderedNotEqual    → IntCC::NotEqual
FloatCC::UnorderedOrEqual   → IntCC::Equal
FloatCC::UnorderedOr*       → signed equivalent
```

**Important difference from the transform**: The existing `convert_fcmp`
returns a fixed-point boolean (0 or 65536, i.e. 0.0 or 1.0 in Q16.16).
It does: `icmp` → `sextend I32` → `imul(_, 65536)`.

The strategy's `emit_cmp` must return the same kind of value that the
_callers_ expect. In the current codegen, `fcmp` returns an i8 boolean
(0 or 1). The callers then use this in `select`, `bint`, etc.

**Decision needed**: Should `emit_cmp` return:
- (a) A raw boolean (i8, 0/1) like float `fcmp` does? The callers already
  handle this.
- (b) A Q32 boolean (i32, 0 or 65536)? This changes caller expectations.

**Recommendation**: Return a raw boolean (i8). The existing codegen call
sites expect `fcmp` to return i8, and `select` works on i8. Returning
a Q32-scaled value would require changing every comparison use site.

If any GLSL expression actually uses the comparison result as a float
(e.g., `float x = step(edge, val)` where step returns 0.0 or 1.0),
that conversion happens at a higher level in the codegen (the `step`
builtin), not in the raw comparison.

So: `emit_cmp` returns `builder.ins().icmp(int_cc, a, b)` — an i8.

## emit_min

```
cmp = icmp(SignedLessThan, a, b)
result = select(cmp, a, b)
```

## emit_max

```
cmp = icmp(SignedGreaterThan, a, b)
result = select(cmp, a, b)
```

## Implementation notes

- The FloatCC → IntCC mapping should be a helper function (or match block)
  inside `Q32Strategy`, since it may be reused.
- NaN/Infinity don't exist in fixed-point. The Ordered/Unordered mappings
  are approximations, same as the transform uses.
