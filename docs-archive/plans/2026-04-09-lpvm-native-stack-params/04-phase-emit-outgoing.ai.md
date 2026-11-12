## Phase 4: Emit - Outgoing Stack Argument Stores

## Scope

Modify `emit_call_direct` and `emit_call_sret` to handle outgoing calls with >8 arguments by storing excess args to the caller's stack area.

## Code Organization Reminders

- Place the store loop after the register move loop
- Keep argument indexing clear (reg args vs stack args)
- Add debug comments in emitted code if helpful

## Implementation Details

### 1. Update emit_call_direct

**File**: `lp-shader/lpvm-native/src/isa/rv32/emit.rs`

Replace the hard error with stack handling:

```rust
/// Direct-return call: args in a0–a7, stack args in caller frame.
fn emit_call_direct(
    &mut self,
    alloc: &Allocation,
    target: &SymbolRef,
    args: &[VReg],
    rets: &[VReg],
    caller_is_sret: bool,
) -> Result<(), NativeError> {
    self.emit_call_preserves_before(alloc, rets, caller_is_sret)?;
    
    let reg_limit = ARG_REGS.len(); // 8 for direct calls
    
    // Move register arguments (first min(args.len(), 8) args)
    for (i, a) in args.iter().enumerate().take(reg_limit) {
        let from = self.use_vreg(alloc, *a, Self::TEMP0)? as u32;
        let to = ARG_REGS[i].hw as u32;
        if from != to {
            self.push_u32(encode_addi(to, from, 0));
        }
    }
    
    // NEW: Store stack arguments (args 8+ to caller arg area)
    if args.len() > reg_limit {
        let base = self.frame.caller_arg_base_from_sp;
        for (i, a) in args.iter().enumerate().skip(reg_limit) {
            let from = self.use_vreg(alloc, *a, Self::TEMP0)? as u32;
            let offset = base + ((i - reg_limit) as i32 * 4);
            let sp = SP.hw as u32;
            self.push_u32(encode_sw(from, sp, offset));
        }
    }
    
    // ... rest of call (auipc+jalr) ...
    let auipc_off = self.code.len();
    let ra = abi::RA.hw as u32;
    self.push_u32(encode_auipc(ra, 0));
    self.push_u32(encode_jalr(ra, ra, 0));
    self.relocs.push(NativeReloc {
        offset: auipc_off,
        symbol: target.name.clone(),
    });
    
    // Move results from return registers
    for (i, r) in rets.iter().enumerate() {
        if i >= RET_REGS.len() {
            return Err(NativeError::TooManyReturns(i + 1));
        }
        let dst = self.def_vreg(alloc, *r, Self::TEMP0)? as u32;
        let src = RET_REGS[i].hw as u32;
        if dst != src {
            self.push_u32(encode_addi(dst, src, 0));
        }
        self.store_def_vreg(alloc, *r, Self::TEMP0);
    }
    
    self.emit_call_preserves_after(alloc, rets, caller_is_sret)?;
    Ok(())
}
```

### 2. Update emit_call_sret

Similar changes for sret calls (reg_limit = 7 since a0 is sret pointer):

```rust
fn emit_call_sret(
    &mut self,
    alloc: &Allocation,
    target: &SymbolRef,
    args: &[VReg],
    rets: &[VReg],
    caller_is_sret: bool,
) -> Result<(), NativeError> {
    self.emit_call_preserves_before(alloc, rets, caller_is_sret)?;
    
    let sret_off = self.frame.sret_slot_offset_from_fp()
        .ok_or(NativeError::MissingSretSlot)?;
    
    // a0 = sret buffer pointer
    let a0 = A0.hw as u32;
    let s0 = S0.hw as u32;
    self.push_u32(encode_addi(a0, s0, sret_off));
    
    let reg_limit = ARG_REGS.len() - 1; // 7 for sret calls (a1-a7)
    
    // Move register arguments to a1-a7
    for (i, a) in args.iter().enumerate().take(reg_limit) {
        let from = self.use_vreg(alloc, *a, Self::TEMP0)? as u32;
        let to = ARG_REGS[i + 1].hw as u32; // +1 for a0 being sret
        if from != to {
            self.push_u32(encode_addi(to, from, 0));
        }
    }
    
    // NEW: Store stack arguments
    if args.len() > reg_limit {
        let base = self.frame.caller_arg_base_from_sp;
        for (i, a) in args.iter().enumerate().skip(reg_limit) {
            let from = self.use_vreg(alloc, *a, Self::TEMP0)? as u32;
            let offset = base + ((i - reg_limit) as i32 * 4);
            let sp = SP.hw as u32;
            self.push_u32(encode_sw(from, sp, offset));
        }
    }
    
    // ... rest (auipc+jalr, load results from sret buffer) ...
}
```

## Tests

Add test verifying store instructions are emitted:

```rust
#[test]
fn emit_call_with_stack_args() {
    // VInst::Call with 10 args
    let call = VInst::Call {
        target: SymbolRef { name: String::from("callee") },
        args: (0..10).map(VReg).collect(),
        rets: vec![VReg(10)],
        callee_uses_sret: false,
        src_op: None,
    };
    
    // Create context with frame that has caller_arg_stack_size >= 8
    let frame = FrameLayout {
        caller_arg_stack_size: 16, // 4 words * 4 bytes = 16 (rounded up)
        caller_arg_base_from_sp: 0,
        // ... other fields ...
    };
    
    let mut ctx = EmitContext::with_frame(frame, false, None);
    
    // Mock allocation with vregs mapped
    let alloc = Allocation {
        vreg_to_phys: (0..11).map(|i| Some(i as u8 + 10)).collect(),
        clobbered: BTreeSet::new(),
        spill_slots: vec![],
        incoming_stack_params: vec![],
    };
    
    // Should not error
    ctx.emit_call_direct(&alloc, &call.target, &call.args, &call.rets, false)
        .expect("emit call with 10 args");
    
    // Should have sw instructions for args 8 and 9
    // Verify by checking code length increased
    assert!(ctx.code.len() > 8); // at least prologue + 2 stores + call
}
```

## Validate

```bash
cd /Users/yona/dev/photomancer/feature/lightplayer-native/lp-shader
cargo test -p lpvm-native --lib
```

Expected: All tests pass, outgoing stack arg handling works.
