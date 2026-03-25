# Phase 5: Memory Ops + Shadow Stack

## Scope

Implement `memory.rs` — shadow stack allocation, `SlotAddr`, `Load`,
`Store`, `Memcpy`. After this phase, functions with LPIR slots (used by
LPFX out-parameters) work correctly.

## Implementation

### Shadow stack design

A mutable WASM global `$sp` (i32) tracks the stack pointer. Only emitted
when at least one function in the module has slots.

The `$sp` initial value is set to a reasonable default (e.g. 65536).
The host (wasmtime / web runtime) provides `env.memory` with sufficient
pages.

### Module-level setup (`emit/mod.rs`)

When any function has non-empty `slots`:
1. Import `env.memory` (1 page minimum).
2. Declare a mutable global `$sp` initialized to 65536.
3. Record the global index for use in function emission.

### Per-function prologue (`emit/func.rs`)

If the function has slots:
```
// Compute frame size: sum of all slot sizes, aligned to 16.
let frame_size = align_up(total_slot_bytes, 16);

global.get $sp
i32.const frame_size
i32.sub
global.set $sp
```

### Per-function epilogue

Before every `Return` op and at function end:
```
global.get $sp
i32.const frame_size
i32.add
global.set $sp
```

The epilogue must be emitted before each `Return` instruction in the
function body (there may be multiple return points).

### Slot offsets

Computed once per function: each slot's offset from `$sp` is the
cumulative sum of preceding slot sizes.

```rust
fn slot_offsets(func: &IrFunction) -> Vec<u32> {
    let mut offsets = Vec::new();
    let mut cur = 0u32;
    for slot in &func.slots {
        offsets.push(cur);
        cur += slot.size;
    }
    offsets
}
```

### Op emission (`emit/memory.rs`)

**`SlotAddr { dst, slot }`**:
```
global.get $sp
i32.const <slot_offsets[slot.0]>
i32.add
local.set dst
```

**`Load { dst, base, offset }`**:
```
local.get base
i32.load offset=<offset> align=2
local.set dst
```

All loads are 4-byte (`i32.load`). LPIR types are 32-bit scalars.

**`Store { base, offset, value }`**:
```
local.get base
local.get value
i32.store offset=<offset> align=2
```

**`Memcpy { dst_addr, src_addr, size }`**:

WASM has `memory.copy` (bulk memory proposal, widely supported):
```
local.get dst_addr
local.get src_addr
i32.const size
memory.copy 0 0
```

If `memory.copy` is not available, fall back to a byte-by-byte loop
(unlikely to be needed — wasmtime supports it).

## Validate

```
cargo check -p lp-glsl-wasm
```

Functions with slots now emit valid WASM with shadow stack management.
