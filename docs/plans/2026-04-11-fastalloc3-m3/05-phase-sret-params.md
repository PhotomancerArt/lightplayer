# Phase 5: Sret Calls + Param Precoloring + Frame Plumbing

## Scope

Handle sret calls (callees returning >2 scalars). Wire `FuncAbi` for param
precoloring (incoming params in ARG_REGS). Plumb sret buffer and call-save
slot info into the frame layout.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Add `max_callee_sret_words` to `FuncAbi`

In `abi/func_abi.rs`, add a field for the maximum sret buffer size this
function's callees need:

```rust
pub struct FuncAbi {
    // ... existing fields ...
    max_callee_sret_words: u32,
}
```

Add getter: `pub fn max_callee_sret_words(&self) -> u32`.

Update `FuncAbi::new_raw` to accept this parameter.

Update `func_abi_rv32` and `ModuleAbi::from_ir_and_sig` to compute and pass
the per-function max callee sret size. This requires knowing which callees
each function calls — for now, use the module-level max (conservative).

### 2. Extend `AllocResult` with frame layout info

The allocator needs to report how many extra frame slots are needed for
call-save and sret:

```rust
pub struct AllocResult {
    pub pinsts: Vec<PInst>,
    pub trace: AllocTrace,
    pub spill_slots: u32,
    pub call_save_slots: u32,      // new: slots for saving caller-saved regs across calls
    pub sret_buffer_words: u32,    // new: sret buffer size in words
}
```

`FrameSetup` and `FrameTeardown` should use
`spill_slots + call_save_slots + sret_buffer_words` as the total frame size.

Update `allocate()` to compute these and include them in the result.
Update the `FrameSetup`/`FrameTeardown` emission to use the total.

### 3. Implement sret call handling in `process_call`

When `callee_uses_sret` is true:

```rust
if callee_uses_sret {
    // a0 = sret buffer address (fp + sret_offset)
    let sret_offset = compute_sret_offset(state);
    state.pinsts.push(PInst::Addi {
        dst: ARG_REGS[0].hw as PReg,
        src: FP_REG,
        imm: sret_offset,
    });

    // Args shift to a1+ (instead of a0+)
    let arg_start_reg = 1; // args begin at ARG_REGS[1]

    // ... same arg placement logic but starting at ARG_REGS[arg_start_reg] ...

    // After call: load results from sret buffer
    for (i, &(rv, preg)) in ret_pregs.iter().enumerate() {
        let offset = sret_offset + (i as i32 * 4);
        state.pinsts.push(PInst::Lw {
            dst: preg,
            base: FP_REG,
            offset,
        });
    }
}
```

The sret buffer offset is relative to FP, computed from the spill area layout:
`sret_offset = -(spill_slots * 4 + call_save_slots * 4 + word_index * 4)` or
similar depending on frame layout. The exact formula depends on how
`FrameSetup` organizes the stack.

### 4. Implement param precoloring

At the end of the backward walk (function entry in execution order), check that
incoming param vregs are in their ABI-specified registers:

In `allocate()`, after the walk and reversal:

```rust
// After pinsts.reverse(), before wrapping with FrameSetup/Teardown:
// Insert param fixup moves at the beginning (after FrameSetup in execution)
let mut param_fixups = Vec::new();
for &(vreg_idx, abi_preg) in func_abi.precolors() {
    let vreg = VReg(vreg_idx as u16);
    // Find where the allocator placed this vreg
    // After the backward walk + reversal, the first instruction's use
    // of this vreg determines its initial register.
    // If the walk assigned it to a different register, emit a Mv.
    //
    // Simple approach: walk the trace to find the first allocation for
    // this vreg, or check if we can determine the initial assignment
    // from the pool state at the end of the backward walk (= function entry).
    if let Some(current_preg) = end_of_walk_pool.home(vreg) {
        if current_preg != abi_preg.hw as PReg {
            param_fixups.push(PInst::Mv {
                dst: current_preg,
                src: abi_preg.hw as PReg,
            });
        }
    }
}
```

Insert these fixup moves right after `FrameSetup` in the final PInst stream.

### 5. Handle S1 preservation for sret callers

When the current function itself is sret (`func_abi.is_sret()`), S1 holds the
sret pointer and must survive across calls. Two options:

a. Treat S1 as a callee-saved register that happens to be pre-occupied —
   save/restore it around calls (like lpvm-native does).
b. Remove S1 from the allocatable set (already done by `func_abi_rv32` when
   sret) so it's never clobbered by the allocator.

Option (b) is already implemented in the ABI (`allocatable.remove(S1)` for sret
functions). But S1 is not in `ALLOC_POOL` either (ALLOC_POOL starts at s2).
So S1 is safe — the allocator never touches it. We just need to save/restore
it around calls if the *current function* is sret:

```rust
if func_abi.is_sret() {
    // Save s1 before call
    state.pinsts.push(PInst::Sw { src: S1_REG, base: FP_REG, offset: s1_save_offset });
    // ... call ...
    // Restore s1 after call
    state.pinsts.push(PInst::Lw { dst: S1_REG, base: FP_REG, offset: s1_save_offset });
}
```

### 6. Remove `AllocError::UnsupportedSret`

With sret handling complete, remove this variant.

## Tests

- Unit test: sret call — verify PInst::Addi for sret ptr, arg shift to a1+, Lw for results.
- Unit test: param precoloring — verify Mv from a0 to allocated reg after FrameSetup.
- Unit test: sret caller with calls — verify s1 save/restore around call.
- Integration: straight-line function with vec4 return type compiles correctly.

## Validate

```bash
cargo test -p lpvm-native-fa
cargo check -p lpvm-native-fa
cargo check -p lp-cli
```
