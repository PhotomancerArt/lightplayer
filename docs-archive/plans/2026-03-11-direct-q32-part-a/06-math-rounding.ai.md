# Phase 6: Update Math and Rounding Call Sites

## `builtins/common.rs`

This file has the most call sites. Many builtin implementations use
inline CLIF instructions for simple math.

### min/max

```rust
// Before:
Type::Float => self.builder.ins().fmin(x, y_scalar),
Type::Float => self.builder.ins().fmax(x, y_scalar),

// After:
Type::Float => self.emit_float_min(x, y_scalar),
Type::Float => self.emit_float_max(x, y_scalar),
```

4 sites (scalar min, vector min, scalar max, vector max).

### abs

```rust
// Before:
Type::Float => self.builder.ins().fabs(val),

// After:
Type::Float => self.emit_float_abs(val),
```

1 site.

### sqrt

```rust
// Before:
result_vals.push(self.builder.ins().sqrt(val));

// After:
result_vals.push(self.emit_float_sqrt(val));
```

1 site.

### floor / ceil

```rust
// Before:
result_vals.push(self.builder.ins().floor(val));
result_vals.push(self.builder.ins().ceil(val));

// After:
result_vals.push(self.emit_float_floor(val));
result_vals.push(self.emit_float_ceil(val));
```

2 sites.

### fract

Uses floor + fsub:

```rust
// Before:
let floored = self.builder.ins().floor(val);
result_vals.push(self.builder.ins().fsub(val, floored));

// After:
let floored = self.emit_float_floor(val);
result_vals.push(self.emit_float_sub(val, floored));
```

2 sites.

### sign

Uses f32const + fcmp:

```rust
// Before:
let zero = self.builder.ins().f32const(0.0);
let one = self.builder.ins().f32const(1.0);
let minus_one = self.builder.ins().f32const(-1.0);
let gt_zero = self.builder.ins().fcmp(FloatCC::GreaterThan, val, zero);
let lt_zero = self.builder.ins().fcmp(FloatCC::LessThan, val, zero);

// After:
let zero = self.emit_float_const(0.0);
let one = self.emit_float_const(1.0);
let minus_one = self.emit_float_const(-1.0);
let gt_zero = self.emit_float_cmp(FloatCC::GreaterThan, val, zero);
let lt_zero = self.emit_float_cmp(FloatCC::LessThan, val, zero);
```

5 sites.

### isinf / isnan signatures

These build ABI signatures with `types::F32`:

```rust
// Before:
sig.params.push(AbiParam::new(types::F32));

// After:
sig.params.push(AbiParam::new(self.float_type()));
```

2 sites.

## `builtins/geometric.rs`

### dot

```rust
// Before:
let mut sum = self.builder.ins().fmul(x_vals[0], y_vals[0]);
let product = self.builder.ins().fmul(x_vals[i], y_vals[i]);
sum = self.builder.ins().fadd(sum, product);

// After:
let mut sum = self.emit_float_mul(x_vals[0], y_vals[0]);
let product = self.emit_float_mul(x_vals[i], y_vals[i]);
sum = self.emit_float_add(sum, product);
```

3 sites per dot call (initial mul, loop mul, loop add).

### cross

6 sites (3 components × fmul + fsub each).

### length

```rust
// Before:
let mut sum_sq = self.builder.ins().fmul(x_vals[0], x_vals[0]);
let sq = self.builder.ins().fmul(x_vals[i], x_vals[i]);
sum_sq = self.builder.ins().fadd(sum_sq, sq);
let result = self.builder.ins().sqrt(sum_sq);

// After — using strategy methods:
let mut sum_sq = self.emit_float_mul(x_vals[0], x_vals[0]);
// ...
let result = self.emit_float_sqrt(sum_sq);
```

~4 sites.

### normalize

```rust
// Before:
result_vals.push(self.builder.ins().fdiv(val, len));

// After:
result_vals.push(self.emit_float_div(val, len));
```

1 site.

### distance

1 site (fsub per component, reuses length).

## Total: ~35 sites across common.rs and geometric.rs

These are all on `&mut self` (CodegenContext), so the convenience methods
work directly.
