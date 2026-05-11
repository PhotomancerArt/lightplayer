# M6: Call Clobbers

## Scope of Work

Implement caller-save register handling around Call instructions. This is the most complex part of the allocator - saving live values in caller-saved registers before the call and restoring them after.

## Files

```
lp-shader/lpvm-native/src/isa/rv32fa/alloc/
└── walk.rs                  # UPDATE: add Call handling
```

## Implementation Details

### 1. Call Handling in `process_instruction`

Add a new arm for `VInst::Call`:

```rust
VInst::Call { args, rets, callee_uses_sret, target, .. } => {
    let mut decision = String::from("Call:");

    // 1. Spill live caller-saved registers
    let caller_saved = caller_saved_int();
    let mut spilled = Vec::new();

    for preg in caller_saved.iter() {
        let hw = preg.hw;
        if let Some(vreg) = self.reg_pool.preg_vreg[hw as usize] {
            if self.live.contains(&vreg) {
                let slot = self.spill.get_or_assign(vreg);
                physinsts.push(PhysInst::Store32 {
                    src: hw,
                    base: FP_REG,
                    offset: -((slot + 1) as i32 * 4),
                });
                self.reg_pool.free(hw);
                spilled.push((vreg, hw, slot));
                decision.push_str(&format!(" spill v{} from {} to [fp-{}]",
                    vreg.0, reg_name(hw), (slot+1)*4));
            }
        }
    }

    // 2. Move args to ABI registers
    let mut arg_regs = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        let want = arg_reg(i, *callee_uses_sret);
        let have = self.ensure_in_reg(*arg, &mut physinsts)?;
        if have != want {
            physinsts.push(PhysInst::Mov32 { dst: want, src: have });
            decision.push_str(&format!(" mov v{}: {}->{}", arg.0, reg_name(have), reg_name(want)));
        }
        arg_regs.push(want);
        // Args are no longer "live" in the sense of needing preservation
        self.live.remove(arg);
    }

    // 3. Emit call
    physinsts.push(PhysInst::Call { target: target.clone() });

    // 4. Reload spilled registers
    for (vreg, old_preg, slot) in spilled {
        // Allocate potentially different register
        let new_preg = self.reg_pool.alloc(vreg)?;
        physinsts.push(PhysInst::Load32 {
            dst: new_preg,
            base: FP_REG,
            offset: -((slot + 1) as i32 * 4),
        });
        if new_preg != old_preg {
            decision.push_str(&format!(" reload v{}: {}->{} (was {})",
                vreg.0, reg_name(old_preg), reg_name(new_preg), reg_name(old_preg)));
        } else {
            decision.push_str(&format!(" reload v{} to {}",
                vreg.0, reg_name(new_preg)));
        }
        self.live.insert(vreg);
        self.reg_pool.touch(new_preg);
    }

    // 5. Assign return values from ABI registers
    for (i, ret) in rets.iter().enumerate() {
        let preg = ret_reg(i);
        // If preg is occupied, evict it
        if let Some(old_vreg) = self.reg_pool.preg_vreg[preg as usize] {
            self.reg_pool.free(preg);
            self.live.remove(&old_vreg);
        }
        self.reg_pool.preg_vreg[preg as usize] = Some(*ret);
        self.reg_pool.touch(preg);
        decision.push_str(&format!(" ret v{}->{}", ret.0, reg_name(preg)));
    }
}
```

### 2. Helper Functions

```rust
/// Get argument register for index.
fn arg_reg(i: usize, callee_uses_sret: bool) -> u8 {
    // If callee uses sret, a0 is reserved for sret pointer
    // Args start at a1 (11) or a0 (10)
    let regs = if callee_uses_sret {
        vec![11, 12, 13, 14, 15, 16, 17]  // a1-a7
    } else {
        vec![10, 11, 12, 13, 14, 15, 16, 17]  // a0-a7
    };
    regs[i]
}

/// Get return register for index.
fn ret_reg(i: usize) -> u8 {
    // a0-a1
    [10, 11][i]
}
```

### 3. SRET Handling

For functions that return via hidden pointer (vec3/vec4):

```rust
// In WalkState::new() - initialize for SRET
if abi.return_method().is_sret() {
    // Reserve s1 (x9) for SRET pointer preservation
    self.reg_pool.preg_vreg[9] = Some(VReg(u32::MAX));  // Mark as reserved
}
```

## Tests

```rust
#[test]
fn test_call_with_no_live_vars() {
    let vinsts = parse_vinsts("
        v0 = IConst32 1
        v1 = Call mod [v0]
        Ret v1
    ").unwrap();

    // Should see: load arg, call, return
    // No spills since nothing live after call
}

#[test]
fn test_call_with_live_caller_saved() {
    let vinsts = parse_vinsts("
        v0 = IConst32 1
        v1 = IConst32 2
        v2 = Call mod [v0]
        v3 = Add32 v1, v2
        Ret v3
    ").unwrap();

    // v1 is live across call
    // Should see: spill v1 before call, reload after
}

#[test]
fn test_call_with_many_args() {
    let vinsts = parse_vinsts("
        v0 = IConst32 0
        v1 = IConst32 1
        v2 = IConst32 2
        v3 = IConst32 3
        v4 = IConst32 4
        v5 = IConst32 5
        v6 = IConst32 6
        v7 = IConst32 7
        v8 = Call many_args [v0,v1,v2,v3,v4,v5,v6,v7]
        Ret v8
    ").unwrap();

    // 8 args should fit in a0-a7
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- call
```

Run the `native-rv32-mod.glsl` test if available (tests `mod()` builtin call).

## Success Criteria

1. Call with no live caller-saved values: no spills
2. Call with live values in caller-saved regs: spill before, reload after
3. Args correctly placed in ABI registers
4. Return values correctly read from ABI registers
5. Trace shows all spill/reload operations clearly
