# Phase 3 Design: Control flow

Plan reference: `2026-03-17-glsl-wasm-part-iii.md` Phase 3

## Goals

1. Implement if/else
2. Implement for loops
3. Implement while loops
4. Implement do-while loops
5. Implement break and continue

---

## 1. If/else

**GLSL:** `if (cond) stmt` or `if (cond) stmt1 else stmt2`.

**WASM:** Structured `if/else/end`. For if-without-else, `else` block can be empty.

```wasm
;; if (cond) then_stmt else else_stmt
(block
  (block
    ;; evaluate cond
    (local.get $cond)
    (i32.eqz)           ;; cond == 0?
    (br_if 1)          ;; if 0, branch to outer block (skip then)
    ;; then_stmt
    ...
    (br 0)             ;; skip else
  )
  ;; else_stmt
  ...
)
```

Simpler structure: WASM `if` can have optional `else`. For `if (c) A else B`:
```wasm
(block
  (block
    (local.get $cond)
    (i32.eqz)
    (br_if 1)          ;; skip then if false
    ;; A
    (br 0)
  )
  ;; B
)
```

Or use `if/else/end` directly when both branches are present:
```wasm
(get cond)
(if
  (then ... then_body ...)
  (else ... else_body ...)
)
```

**wasm-encoder:** `sink.if_(BlockType::Empty)` then instructions for then, `sink.else_()`, instructions for else, `sink.end()`.

**No value:** If/else is a statement, not an expression. So `BlockType::Empty`.

---

## 2. For loops

**GLSL:** `for (init; cond; update) body`

**WASM pattern:** `block` + `loop`. Init runs before the block. Loop contains: condition check (br_if to exit), body, update, br back to loop.

```wasm
;; init
...
(block $exit
  (loop $loop
    ;; cond
    (local.get $i)
    (local.get $n)
    (i32.ge_s)
    (br_if $exit)      ;; exit if cond false
    ;; body
    ...
    ;; update
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $loop)
  )
)
```

**Branch depths:** `br 0` = exit loop (go to block). `br 1` = exit block. So `br_if 1` with condition = exit block. `br 0` = repeat loop.

Actually in WASM, branch targets are block/loop indices. When we have:
```
block 0
  loop 1
    ...
    br_if 0   ;; branch to block 0 (exit)
    ...
    br 1      ;; branch to loop 1 (continue)
  end
end
```
So: `br_if 0` exits the block (out of loop). `br 1` goes back to loop header.

---

## 3. While loops

**GLSL:** `while (cond) body`

**WASM:** Same as for, but no init or update. Loop: check cond (br_if exit), body, br loop.

```wasm
(block $exit
  (loop $loop
    (local.get $cond)
    (i32.eqz)
    (br_if $exit)
    ;; body
    ...
    (br $loop)
  )
)
```

---

## 4. Do-while loops

**GLSL:** `do body while (cond);`

**WASM:** Loop runs at least once. At end, check cond; if true, br back to loop; else fall through.

```wasm
(block $exit
  (loop $loop
    ;; body
    ...
    ;; cond
    (local.get $cond)
    (i32.eqz)
    (br_if $exit)      ;; if false, exit
    (br $loop)         ;; if true, continue
  )
)
```

---

## 5. Break and continue

**Break:** Exit the innermost loop (or switch). In loop structure, `br` to the block that wraps the loop.

**Continue:** Jump to loop header (next iteration). In loop structure, `br` to the loop.

**Nesting:** For nested loops, we need to track block/loop depths. When we emit `break`, we `br` to the appropriate outer block. When we emit `continue`, we `br` to the appropriate loop.

**Implementation:** Maintain a stack of (block_depth, loop_depth) or (block_label, loop_label) during emission. On break: `br block_depth`. On continue: `br loop_depth` (to the loop, which is one level deeper than the block that contains it).

WASM branch targets: `br n` pops n frames. So:
- `block` (exit target) + `loop` (continue target). From inside body: `br 0` = exit loop (to block). `br 1` = ... actually `br` takes a label index. Label 0 = innermost. So inside loop body: label 0 = loop, label 1 = block. `br 0` = go to loop (continue). `br 1` = go to block (break).

Correct: In WASM, `br labelidx` branches to the label. The block creates label 0. The loop creates label 1 (inside the block). So from inside the loop body: `br 1` exits (to block end). `br 0` continues (to loop start).

---

## 6. Statement emission changes

**Current:** `emit_statement_to_sink` handles `Simple` (decl, expr, jump) and `Compound`. Does not handle `Selection` or `Iteration`.

**New:** Add `SimpleStatement::Selection(sel)` → emit_if_stmt. Add `SimpleStatement::Iteration(iter)` → emit_loop_stmt (dispatch to for/while/do-while).

---

## 7. Condition evaluation

**Condition::Expr(expr):** Emit expr, validate bool, use value.

**Condition::Assignment:** Declare variable, emit initializer, use value. (Cranelift has this in `emit_condition`.) For WASM, allocate local, emit init, local.set, then local.get for condition value.

---

## File change summary

| File | Changes |
|------|---------|
| `codegen/stmt/mod.rs` | Handle Selection, Iteration in emit_simple_statement |
| `codegen/stmt/if.rs` | New: emit_if_stmt (if/else/end) |
| `codegen/stmt/loop_for.rs` | New: emit_for_loop (block+loop, init/cond/update) |
| `codegen/stmt/loop_while.rs` | New: emit_while_loop |
| `codegen/stmt/loop_do_while.rs` | New: emit_do_while_loop |
| `codegen/stmt/jump.rs` | Handle Break, Continue (need block/loop depth tracking) |
| `codegen/context.rs` | Add loop_depth/block_depth stack for break/continue |

---

## Validation

- `control/if/*`, `control/if_else/*`
- `control/for/*`, `control/while/*`, `control/do_while/*`
- Tests with break/continue in nested loops
