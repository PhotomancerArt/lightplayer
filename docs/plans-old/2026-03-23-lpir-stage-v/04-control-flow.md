# Phase 4: Control Flow

## Scope

Implement `control.rs` — structured control flow emission for WASM.
After this phase, programs with if/else, loops, switch, break, continue,
br_if_not, and return produce valid WASM.

## Implementation

### `emit/control.rs`

The emitter walks the LPIR body linearly. Control flow marker ops
(`IfStart`, `Else`, `LoopStart`, `End`, etc.) translate to WASM
structured control instructions. The emitter maintains a control stack
to compute `br` target depths.

### Control stack

```rust
enum CtrlEntry {
    If,
    Else,
    /// Three WASM constructs: outer block, loop, inner block.
    /// break targets outer block, continue targets inner block end.
    Loop {
        /// WASM depth after pushing the outer block.
        break_depth: u32,
        /// WASM depth after pushing the inner block (body).
        body_depth: u32,
        /// PC of the LoopStart (for continuing_offset lookup).
        loop_start_pc: usize,
    },
    SwitchOuter,
    SwitchCase,
}
```

### If / Else / End

**`IfStart { cond, .. }`**:
```
local.get cond
if
```
Push `CtrlEntry::If`. Depth += 1.

**`Else`**:
```
else
```
Pop `If`, push `Else`.

**`End`** (closing an if or else):
```
end
```
Pop `If` or `Else`. Depth -= 1.

### Loop

**`LoopStart { continuing_offset, end_offset }`**:

Emit three WASM constructs:
```
block          // outer: break target
  loop         // loop header: continue re-enters here
    block      // inner: body, continue exits here
```
Push `CtrlEntry::Loop { break_depth, body_depth, loop_start_pc }`.
Depth += 3.

The body ops are emitted naturally. When `pc == continuing_offset`
(and continuing_offset != start+1), the inner block is closed:
```
    end        // close inner block
```
Depth -= 1. Then continuing ops are emitted.

**`End`** (closing a loop):

If there's still an inner block open (no separate continuing section),
close it first:
```
    end        // close inner block (if not already closed by continuing)
```

Then:
```
    br 0       // unconditional jump back to loop header
  end          // close loop
end            // close outer block
```
Pop `CtrlEntry::Loop`. Adjust depth.

### Break / Continue / BrIfNot

**`Break`**:
- Find innermost `Loop` in control stack.
- Compute relative depth to the outer block: `depth - break_depth`.
- Emit `br <relative_depth>`.

**`Continue`**:
- Find innermost `Loop` in control stack.
- Compute relative depth to the inner block: `depth - body_depth`.
- Emit `br <relative_depth>`.

Note: `Continue` jumps to the end of the inner block, which falls
through into the continuing section, then hits `br 0` back to loop top.

**`BrIfNot { cond }`**:
- This is "break if not cond" — break when cond is false.
- Find innermost `Loop`.
- Emit: `local.get cond`, `i32.eqz`, `br_if <depth_to_outer_block>`.

### Switch

**`SwitchStart { selector, end_offset }`**:

WASM doesn't have a direct switch. Use nested blocks + `br_table`:

1. Count the cases by scanning forward to find all `CaseStart` and
   `DefaultStart` ops before the matching `End`.
2. Emit nested `block` constructs (one per case + one for default).
3. Emit `br_table` with the selector mapping each value to the
   correct block depth.
4. Push appropriate `CtrlEntry` items.

This is the most complex part. For an initial implementation, a simpler
approach using chained `if/else` may be acceptable:

```
// For each case:
local.get selector
i32.const case_value
i32.eq
if
    <case body>
    br <out>
end
// default:
<default body>
```

### Return

**`Return { values }`**:
- For each return value: `local.get vreg`.
- If the function has slots (shadow stack), emit epilogue first.
- Emit `return`.

## Validate

```
cargo check -p lps-wasm
```

Programs with control flow now emit valid WASM. Combined with Phases 2-3,
integer and Q32 arithmetic with control flow should work.
