# Phase 3: Control Flow

## Scope

Write one spec chapter:
- `docs/lpir/04-control-flow.md` — All structured control flow ops,
  their semantics, nesting rules, and the mapping from Naga statements.

## Reminders

- This is a spec-writing phase, no Rust code.
- Be precise about nesting and scoping rules.
- Document the WASM emission pattern for each construct.
- Document the Naga → LPIR mapping for each statement type.

## Implementation details

### Control flow ops

#### if / else

```
if v0 {
  ...body...
}

if v0 {
  ...accept...
} else {
  ...reject...
}
```

- `v0` must be `i32` (`0` = false, nonzero = true).
- The `else` clause is optional.
- VRegs defined inside branches are visible after the `if` (non-SSA —
  they may or may not have been assigned depending on which branch ran).
- No "else if" syntax — nest explicitly:
  ```
  if v0 {
    ...
  } else {
    if v1 {
      ...
    }
  }
  ```

WASM mapping: `If(Empty)` → accept → `Else` → reject → `End`.

Naga mapping: `Statement::If { condition, accept, reject }` → emit
condition VReg, then `if`/`else` with lowered bodies.

#### loop

```
loop {
  ...body...
}
```

- Infinite loop. Exit via `break` or `return`.
- `continue` jumps to the loop header (re-enters body from the top).
- `br_if_not` provides conditional exit.

WASM mapping:
```
Block(Empty)          ; break target
  Loop(Empty)         ; loop header (continue target)
    ...body...
    Br(0)             ; unconditional branch to loop header
  End
End
```

Naga mapping: `Statement::Loop { body, continuing, break_if }`.

The Naga loop has a `continuing` block and an optional `break_if` condition.
The lowering handles the Naga-specific do-while pattern
(`split_do_while_trailing_guard`) by restructuring the body so that
`continue` skips the trailing guard.

Document the Naga loop → LPIR loop mapping in detail:
```
; Naga: Loop { body, continuing, break_if }
loop {
  ...body...           ; from body statements
  ...continuing...     ; from continuing statements
  br_if_not v_cond     ; from break_if (negated: break if NOT condition)
  continue
}
```

#### break

```
break
```

- Exits the innermost enclosing `loop`.
- Illegal outside a loop.

WASM mapping: `Br(depth)` to the loop's outer `Block`.

Naga mapping: `Statement::Break`.

#### continue

```
continue
```

- Jumps to the header of the innermost enclosing `loop`.
- Illegal outside a loop.

WASM mapping: `Br(depth)` to the `Loop` instruction.

Naga mapping: `Statement::Continue`.

#### br_if_not

```
br_if_not v0
```

- Conditional break: exits the innermost `loop` if `v0` (`i32`) is `0`.
- Equivalent to `if (!v0) break`, but as a single op.
- Only valid inside a `loop`.
- The "not" naming reflects the loop-guard idiom: "keep looping while
  condition holds, break if not."

WASM mapping: `local.get v0` → `i32.eqz` → `br_if` to the loop's outer
`Block`. (WASM `br_if` branches on nonzero, so `i32.eqz` inverts the
condition to match `br_if_not` semantics.)

This op exists because Naga's `break_if` pattern in loops is common
and a single op avoids an extra `if` + `break` nesting level.

#### switch

```
switch v0 {
  case 0 {
    ...
  }
  case 1 {
    ...
  }
  case 5 {
    ...
  }
  default {
    ...
  }
}
```

- `v0` must be `i32`.
- Case values are non-negative integer constants; duplicates are an error.
- **No fall-through**: each case arm is independent. After executing a
  matching case body, control falls to after the `switch`.
- `default` is optional. If absent and no case matches, control falls to
  after the `switch` (no-op).
- `break` and `continue` inside case bodies target enclosing `loop`s, not
  the `switch`. The switch is not a loop construct.

WASM mapping (dense cases):
```
Block(Empty)                   ; end-of-switch target
  Block(Empty)                 ; default target
    Block(Empty)               ; case N target
      ...
        Block(Empty)           ; case 0 target
          local.get v0
          BrTable [0, 1, ..., N, default]
        End                    ; case 0
        ...case 0 body...
        Br(end)
      End                      ; case 1
      ...case 1 body...
      Br(end)
    End                        ; case N
    ...case N body...
    Br(end)
  End                          ; default
  ...default body...
End                            ; end-of-switch
```

For sparse cases, the emitter may normalize the selector (subtract min,
gap-fill with default), or fall back to an `if`/`else` chain. This is an
emitter optimization detail — the IR always uses `switch`.

Cranelift mapping: `cranelift_frontend::Switch` utility (automatically
selects jump table vs binary search based on case density).

Naga mapping: `Statement::Switch { selector, cases }`. Each
`SwitchCase { value: I32(n), body, fall_through }` maps to `case n { body }`.
`SwitchValue::Default` maps to `default { body }`. Fall-through cases
(`fall_through: true`) are merged by the Naga → LPIR lowering: a
fall-through case's body is prepended to the next case's body.

#### return

```
return v0            ; return a value
return               ; return void
```

- Returns from the current function.
- If the function has a return type, a VReg must be provided.
- If the function is void, no VReg.

WASM mapping: emit return value (if any), then `Return`.

Naga mapping: `Statement::Return { value }`.

### Nesting and scoping rules

Document:
- Loops, ifs, and switches can nest arbitrarily inside each other.
- `break` and `continue` target the innermost enclosing `loop` only
  (not `switch` — switch is not a loop construct).
- `br_if_not` targets the innermost enclosing `loop` only.
- VReg scope is the entire function (flat, not block-scoped). A VReg
  defined inside an `if` branch or `switch` case is accessible after
  the construct, but its value is only defined if that branch executed.
- There is no implicit fall-through or phi-node merging. The non-SSA
  model means VRegs defined before a branch retain their value in
  branches that don't reassign them.

### Control flow examples

Include a comprehensive set of examples:

1. Simple conditional:
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

2. If/else:
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

3. Loop with counter:
```
func @sum_to_n(v0:i32) -> i32 {
  v1:i32 = iconst.i32 0           ; accumulator
  v2:i32 = iconst.i32 0           ; counter
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

4. Nested loops:
```
func @nested(v0:i32, v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  v3:i32 = iconst.i32 0           ; outer counter
  loop {
    v4:i32 = ilt_s v3, v0
    br_if_not v4
    v5:i32 = iconst.i32 0         ; inner counter
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

5. Switch:
```
func @switch_example(v0:i32) -> f32 {
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

6. Early return:
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

## Validate

Review the section for:
- Every control flow construct has syntax, semantics, and WASM mapping.
- Nesting rules are explicit and unambiguous.
- VReg scoping in branches and switch cases is clearly documented.
- Naga → LPIR mapping covers all statement types.
- Examples cover: simple if, if/else, loop with br_if_not, nested loops,
  switch, early return.
- Cross-reference with the current WASM emitter's `emit_stmt` to ensure
  nothing is missing.
