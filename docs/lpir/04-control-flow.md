# Control Flow

LPIR uses structured control flow aligned with Naga and WebAssembly: nested `if`, `loop`, `switch`, and jumps (`break`, `continue`, `br_if_not`, `return`). There is no explicit control-flow graph or basic-block representation in the IR text.

## `if` / `else`

```
if v_cond {
  ... then ...
}

if v_cond {
  ... accept ...
} else {
  ... reject ...
}
```

| | |
|--|--|
| Condition | `v_cond` must be `i32`. The value `0` is false; any nonzero value is true. |
| `else` | Optional. There is no `else if` form; nest a second `if` inside the `else` branch. |
| VRegs after the construct | Definitions that occur only on one branch are visible after the `if` (non-SSA). Values of such registers are only defined if the branch that assigned them ran. |

**WebAssembly:** `block` with `if` (empty type) for the then arm, optional `else`, then `end`.

**Naga:** `Statement::If { condition, accept, reject }` — condition lowers to an `i32` VReg; bodies lower into the respective arms.

## `loop`

```
loop {
  ... body ...
}
```

| | |
|--|--|
| Semantics | Infinite repetition of `body` until `break` or `br_if_not` exits the loop, or `return` exits the function. |
| `continue` | Transfers control to the loop header (next iteration). |
| Exit | `break` or `br_if_not` leaves the innermost enclosing loop; `return` leaves the function. |

**WebAssembly:** Outer `block` (break target), inner `loop` (continue target), body, unconditional backward branch to the loop, matching `end` instructions. A common shape is: `Block` → `Loop` → … → `br 0` to the loop → `end` → `end`.

**Naga:** `Statement::Loop { body, continuing, break_if }`. Lowering places `body` and `continuing` statements in order inside the `loop`, then emits `br_if_not` from the lowered `break_if` when present, then `continue` so the header runs each iteration. Exact restructuring for Naga’s do-while-style guards is handled in the lowering (for example splitting a trailing guard so `continue` skips it).

## `break`

```
break
```

Exits the innermost enclosing `loop`. A `break` not nested in any `loop` is invalid.

**WebAssembly:** Branch to the instruction after the innermost `loop`’s surrounding `block` (the break target).

**Naga:** `Statement::Break`.

## `continue`

```
continue
```

Jumps to the header of the innermost enclosing `loop`. Invalid outside a `loop`.

**WebAssembly:** Branch to the innermost `loop` instruction.

**Naga:** `Statement::Continue`.

## `br_if_not`

```
br_if_not v0
```

| | |
|--|--|
| Semantics | If `v0` (`i32`) is `0`, exit the innermost enclosing `loop` as `break` would. If `v0` is nonzero, fall through. |
| Placement | Only valid inside a `loop`. |
| Naming | Encodes the guard idiom: keep iterating while a condition holds; break when it does not. |

**WebAssembly:** `local.get v0`, `i32.eqz`, `br_if` to the same target as `break` (nonzero on stack branches; `eqz` inverts the test).

**Naga:** Used when lowering `break_if` and similar loop exits.

## `switch`

```
switch v_sel {
  case 0 {
    ... arm0 ...
  }
  case 1 {
    ... arm1 ...
  }
  default {
    ... default_arm ...
  }
}
```

| | |
|--|--|
| Selector | `v_sel` is `i32`. |
| Case labels | Integer constants; duplicate values are invalid. |
| Fall-through | None. After a matching arm runs, control continues after the whole `switch`. |
| `default` | Optional. If omitted and no case matches, control skips to after the `switch`. |
| `break` / `continue` inside arms | Target the innermost enclosing `loop`, not the `switch`. |

**WebAssembly:** Emitters use nested blocks and `br_table` for dense case sets; sparse sets may use normalization or an `if`/`else` chain. The IR form is always `switch` as above; the opcode sequence is an implementation detail.

**Cranelift:** Lowering may use `cranelift_frontend::Switch` (jump table vs search, depending on case density).

**Naga:** `Statement::Switch { selector, cases }`. `SwitchCase` entries with `fall_through: true` are merged during lowering (earlier case bodies concatenated into the next case) so LPIR arms remain independent.

## `return`

```
return v0
return
```

| | |
|--|--|
| With value | Required when the function has a non-void return type; `v0`’s type must match. |
| Without value | Only for void functions. |

**WebAssembly:** Push return values if any, then `return`.

**Naga:** `Statement::Return { value }`.

## Nesting and scoping

- `if`, `loop`, and `switch` may nest arbitrarily.
- `break`, `continue`, and `br_if_not` apply to the innermost enclosing `loop` only, never to `switch`.
- VReg names are scoped to the entire function. Assignments inside a branch or case are visible afterward under the non-SSA rules above; there is no automatic merge or φ-node.
- No implicit fall-through between `switch` arms.

## Examples

### 1. Simple conditional (`abs`)

```
func @abs(v0:f32) -> f32 {
  v1:f32 = fconst.f32 0.0
  v2:i32 = flt v0, v1
  if v2 {
    v0 = fneg v0
  }
  return v0
}
```

### 2. `if` / `else` (`max`)

```
func @max(v0:f32, v1:f32) -> f32 {
  v2:i32 = fgt v0, v1
  if v2 {
    return v0
  } else {
    return v1
  }
}
```

### 3. Loop with counter and `br_if_not` (`sum_to_n`)

```
func @sum_to_n(v0:i32) -> i32 {
  v1:i32 = iconst.i32 0
  v2:i32 = iconst.i32 0
  loop {
    v3:i32 = ilt_s v2, v0
    br_if_not v3
    v1 = iadd v1, v2
    v4:i32 = iconst.i32 1
    v2 = iadd v2, v4
    continue
  }
  return v1
}
```

### 4. Nested loops

```
func @nested(v0:i32, v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  v3:i32 = iconst.i32 0
  loop {
    v4:i32 = ilt_s v3, v0
    br_if_not v4
    v5:i32 = iconst.i32 0
    loop {
      v6:i32 = ilt_s v5, v1
      br_if_not v6
      v2 = iadd v2, v5
      v7:i32 = iconst.i32 1
      v5 = iadd v5, v7
      continue
    }
    v8:i32 = iconst.i32 1
    v3 = iadd v3, v8
    continue
  }
  return v2
}
```

### 5. `switch` with cases and `default`

```
func @dispatch(v0:i32) -> f32 {
  v1:f32 = fconst.f32 0.0
  switch v0 {
    case 0 {
      v1 = fconst.f32 1.0
    }
    case 1 {
      v1 = fconst.f32 2.0
    }
    case 2 {
      v1 = fconst.f32 4.0
    }
    default {
      v1 = fconst.f32 -1.0
    }
  }
  return v1
}
```

### 6. Early return

```
func @early_return(v0:f32) -> f32 {
  v1:f32 = fconst.f32 0.0
  v2:i32 = flt v0, v1
  if v2 {
    v3:f32 = fneg v0
    return v3
  }
  return v0
}
```
