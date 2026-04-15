# Phase 3: Entry Parameter Moves

## Scope

Record entry moves when parameters get evicted from their ABI registers.

## Implementation

### 1. Track param locations in walk.rs

Modify `walk_linear` to track entry param final locations:

```rust
pub fn walk_linear(...) -> Result<AllocOutput, AllocError> {
    // ... existing setup ...
    
    // Track entry params and their ABI regs
    let entry_precolors: Vec<(VReg, PReg)> = /* existing */;
    
    // ... backward walk ...
    
    // After walk: check where each param ended up
    let mut entry_edits: Vec<(EditPoint, Edit)> = Vec::new();
    for (vreg, abi_reg) in entry_precolors {
        let final_alloc = if let Some(preg) = pool.home(vreg) {
            Alloc::Reg(preg)
        } else if let Some(slot) = spill.has_slot(vreg) {
            Alloc::Stack(slot)
        } else {
            // Param never used, no need to track
            continue;
        };
        
        let abi_alloc = Alloc::Reg(abi_reg);
        
        // If param moved from ABI reg, record entry move
        if final_alloc != abi_alloc {
            entry_edits.push((
                EditPoint::Before(0),
                Edit::Move {
                    from: abi_alloc,
                    to: final_alloc,
                },
            ));
        }
    }
    
    // Combine entry edits with other edits
    entry_edits.extend(edits);
    let final_edits = entry_edits;
    
    Ok(AllocOutput {
        // ...
        edits: final_edits,
        // ...
    })
}
```

### 2. Trace-based rendering (implemented)

Entry moves and entry precolors are shown via the allocator trace system:

- Entry precolors: `; entry: vN -> xN` in metadata header (shows ABI register assignment)
- Entry moves: `; entry_move: xN -> tM` in metadata (when param forced out of ABI reg)
- Entry spills: `; entry_spill: xN -> slotM` in metadata (when param goes to stack)

Per-instruction trace (`; trace: alloc: vN -> tM`) shows fresh allocations during backward walk.

### 3. Filetest location

Param filetests live in `lp-shader/lpvm-native/filetests/param/`:
- `stays_in_reg.lpir` — single param stays in ABI reg
- `evicted_to_reg.lpir` — two params with limited pool
- `spilled_at_entry.lpir` — same setup (true spills need call clobbers)
- `multi_param_mixed.lpir` — four params with pool_size=2

### 4. Filetest coverage

Filetests in `lp-shader/lpvm-native/filetests/param/` cover param scenarios:

- `stays_in_reg.lpir` — param stays in ABI reg (a1), no entry move
- `evicted_to_reg.lpir` — params stay in ABI regs (limited pool but no eviction pressure on ABI regs)
- `spilled_at_entry.lpir` — same as evicted (need call clobbers to actually trigger entry moves)
- `multi_param_mixed.lpir` — multiple params, some stay in ABI regs

**Note**: True entry moves (`; entry_move:`) only appear when a param is forced out of its ABI register. With the current allocator, ABI regs (a0-a7) are outside the `ALLOC_POOL`, so they cannot be LRU-evicted. Entry moves will first appear in **M3 (Calls)** when call clobbers force params out of ABI regs.

## Validation

```bash
# Filetests
cargo test -p lpvm-native --test filetests

# All param tests
cargo test -p lpvm-native param
```

## Success Criteria

- [x] Entry precolors tracked in `walk.rs` (`entry_precolors`)
- [x] Entry moves recorded when params end up different from ABI regs (code in place, triggers on call clobbers in M3)
- [x] Trace shows `; entry: vN -> xN` and `; entry_move: xN -> tM` in metadata
- [x] Filetests in `filetests/param/` cover param scenarios
- [x] Trace interleaved in snapshot output for debugging
