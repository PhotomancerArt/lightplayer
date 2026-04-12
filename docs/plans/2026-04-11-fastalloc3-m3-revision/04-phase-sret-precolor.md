# Phase 4: Sret Calls + Param Precoloring

## Scope of phase

Implement sret call handling and param precoloring. These are independent
features that extend the existing functionality.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Sret Call Handling

Sret (struct return) calls are used for returning vec3/vec4 (>2 scalars).
The caller:
1. Allocates an sret buffer on its stack
2. Passes buffer address in a0
3. Shifts other args to a1+, a2+, etc.
4. After call, loads results from the buffer

Update `process_call` in `walk.rs`:

```rust
fn process_call(
    state: &mut WalkState<'_>,
    idx: usize,
    vinst: &VInst,
    vreg_pool: &[VReg],
    func_abi: &crate::abi::FuncAbi,
    sret_buffer_offset: Option<u32>,  // NEW: offset from fp for sret buffer
) -> Result<(), AllocError> {
    // ... existing code ...

    if callee_uses_sret {
        let offset = sret_buffer_offset.ok_or(AllocError::MissingSretBuffer)?;
        // Step 0: Set up sret pointer in a0
        // In backward walk: this is done "after" the call (first in stream)
        state.pinsts.push(PInst::Lw {
            dst: 10, // a0
            base: FP_REG,
            offset: -(offset as i32),
        }); // Load sret buffer addr from saved location
        // Actually this is complex — the sret buffer addr needs to be
        // stored somewhere before the call...

        // Simpler: the sret buffer is at a known fp offset.
        // Before call: a0 = fp - sret_offset
        // In backward: this is done after call processing
        // But we need to compute fp - offset, which is addi

        // Actually let's not overcomplicate. For now, just support
        // the case where the sret buffer is passed and we load results.

        // Steps for sret in backward order:
        // 1. Load results from sret buffer (post-call in forward)
        // 2. Call
        // 3. Set up sret buffer pointer in a0 (pre-call in forward)
        //    In backward: this is done "after" loading results
        // 4. Shifted args in a1+, a2+ (pre-call in forward)
        //    In backward: done after sret pointer setup

        // Implement the full sret handling...
    }
}
```

Actually, sret handling needs more design. The sret buffer needs to be
allocated in the caller's frame, and the buffer address passed in a0.

For now, the simplest approach:
1. Add `sret_buffer_bytes` to `FuncAbi` or pass it to `allocate()`
2. In `allocate()`, reserve frame space for the sret buffer
3. In `process_call` with sret:
   - Before call: `addi a0, fp, -sret_offset` (set up sret pointer)
   - Args go in a1+, a2+, etc. instead of a0+, a1+, etc.
   - After call: load return values from the sret buffer

The backward walk order:
```
1. Load results from sret buffer (post-call)
2. Call
3. Set up sret pointer (addi a0, fp, -offset)
4. Move args to a1+, a2+ (shifted by one)
5. Spill caller-saved (pre-call)
```

### 2. Param Precoloring

In `fa_alloc/mod.rs`, before starting the backward walk:

```rust
pub fn allocate(lowered: &LoweredFunction, func_abi: &FuncAbi) -> Result<AllocResult, AllocError> {
    // ... existing setup ...

    let mut state = walk::WalkState::new(num_vregs, &lowered.symbols);

    // Pre-seed pool with param vregs in their ARG_REGs
    for (vreg_idx, preg) in func_abi.precolors() {
        // Skip if this param vreg has no uses (dead)
        // For now, just seed all precolors
        state.pool.alloc_fixed(preg.hw, VReg(vreg_idx as u16));
    }

    // ... rest of allocate ...
}
```

The precolors are from `FuncAbi::precolors()` which returns `[(vreg_idx, PReg)]`
for function parameters.

If a param is evicted during the walk (register pressure), the normal
spill/reload handles it. The param will be reloaded when needed.

### 3. Sret Buffer Plumbing

Add `max_callee_sret_bytes` to `FuncAbi`:

```rust
pub struct FuncAbi {
    // ... existing fields ...
    pub max_callee_sret_bytes: u32,  // NEW: max sret buffer needed for callees
}
```

This is computed during ABI construction and used to reserve frame space.

## Tests

- Unit test: param precoloring seeds pool correctly
- Unit test: sret call emits correct sequence
- Integration: function with vec3 return type works

## Validate

```bash
cargo test -p lpvm-native-fa
cargo check -p lpvm-native-fa
```
