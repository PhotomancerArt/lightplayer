# Phase 5: Implement Caller-Side Sret in Emission

## Scope of Phase

Update emission to handle caller-side sret for `VInst::Call`. This is the most complex phase, involving:
1. Threading `ModuleAbi` through to emission functions
2. Using `callee_uses_sret` flag to choose emission path
3. Implementing sret path: buffer address in a0, args in a1-a7, load results from buffer

## Code Organization Reminders

- Place helper methods at the bottom of `EmitContext`
- Keep VInst::Call emission in `emit_vinst` method
- Add tests at the bottom of the test module
- Use existing spill handling patterns for loading from buffer

## Implementation Details

### File: `lp-shader/lpvm-native/src/isa/rv32/emit.rs`

**Update `emit_function_bytes` signature:**

```rust
/// Emit one function to RV32 bytes (and relocations).
pub fn emit_function_bytes(
    func: &lpir::IrFunction,
    module_abi: &ModuleAbi,      // NEW: replaces fn_sig parameter
    float_mode: lpir::FloatMode,
    debug_info: bool,
) -> Result<EmittedFunction, NativeError> {
    let vinsts = crate::lower::lower_ops(func, &module_abi.as_ir_module()?, float_mode)?;  // UPDATE
    let slots = func.total_param_slots() as usize;
    
    // Get this function's signature and ABI
    let func_sig = module_abi.func_sig_for(func.name.as_str())  // NEW helper
        .ok_or_else(|| NativeError::MissingFunctionSig(func.name.clone()))?;
    let func_abi = func_abi_rv32(&func_sig, slots);
    
    let alloc = GreedyAlloc::new().allocate_with_func_abi(func, &vinsts, &func_abi)?;
    let is_leaf = !vinsts.iter().any(|v| v.is_call());
    let is_sret = func_abi.is_sret();
    
    // Compute used callee-saved registers
    let mut used_callee_saved = PregSet::EMPTY;
    for preg_opt in &alloc.vreg_to_phys {
        if let Some(preg) = preg_opt {
            let p = PReg::int(*preg);
            if callee_saved_int().contains(p) {
                used_callee_saved.insert(p);
            }
        }
    }
    if is_sret {
        used_callee_saved.insert(S1);
    }
    
    // NEW: Get max sret bytes for caller-side sret slot
    let caller_sret_bytes = module_abi.max_callee_sret_bytes();
    
    // Create frame with spill count and caller sret bytes
    let frame = FrameLayout::compute(
        &func_abi,
        alloc.spill_count(),
        used_callee_saved,
        &[],
        is_leaf,
        caller_sret_bytes,  // NEW
    );
    
    let mut ctx = EmitContext::with_frame(frame, debug_info);
    ctx.emit_prologue(is_sret);
    for v in &vinsts {
        ctx.emit_vinst(v, &alloc, is_sret)?;
    }
    ctx.resolve_branch_fixups()?;
    ctx.emit_epilogue();
    Ok(EmittedFunction {
        code: ctx.code,
        relocs: ctx.relocs,
        debug_lines: ctx.debug_lines,
    })
}
```

**Update `VInst::Call` emission in `emit_vinst`:**

```rust
VInst::Call {
    target,
    args,
    rets,
    callee_uses_sret,
    ..
} => {
    if *callee_uses_sret {
        self.emit_call_sret(alloc, target, args, rets)?;
    } else {
        self.emit_call_direct(alloc, target, args, rets)?;
    }
}
```

**Add helper methods for call emission:**

```rust
impl EmitContext {
    /// Emit a call to a function with direct return (results in a0-a1).
    fn emit_call_direct(
        &mut self,
        alloc: &Allocation,
        target: &SymbolRef,
        args: &[VReg],
        rets: &[VReg],
    ) -> Result<(), NativeError> {
        if args.len() > ARG_REGS.len() {
            return Err(NativeError::TooManyArgs(args.len()));
        }
        
        // Move args to a0-a7
        for (i, a) in args.iter().enumerate() {
            let from = self.use_vreg(alloc, *a, Self::TEMP0)? as u32;
            let to = ARG_REGS[i].hw as u32;
            if from != to {
                self.push_u32(encode_addi(to, from, 0));
            }
        }
        
        // Emit auipc+jalr
        let auipc_off = self.code.len();
        let ra = abi::RA.hw as u32;
        self.push_u32(encode_auipc(ra, 0));
        self.push_u32(encode_jalr(ra, ra, 0));
        self.relocs.push(NativeReloc {
            offset: auipc_off,
            symbol: target.name.clone(),
        });
        
        // Move results from a0-a1 to destination vregs
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
        
        Ok(())
    }
    
    /// Emit a call to a function with sret return.
    /// 
    /// Steps:
    /// 1. Compute sret buffer address (fp + sret_slot_offset)
    /// 2. Move buffer address to a0
    /// 3. Move user args to a1-a7 (shifted by 1)
    /// 4. Emit auipc+jalr
    /// 5. Load return values from buffer into result vregs
    fn emit_call_sret(
        &mut self,
        alloc: &Allocation,
        target: &SymbolRef,
        args: &[VReg],
        rets: &[VReg],
    ) -> Result<(), NativeError> {
        let max_args = ARG_REGS.len() - 1;  // a1-a7 = 7 regs (a0 is sret pointer)
        if args.len() > max_args {
            return Err(NativeError::TooManyArgs(args.len()));
        }
        
        // Step 1 & 2: Compute sret buffer address and put in a0
        let sret_offset = self.frame.sret_slot_offset_from_fp()
            .ok_or_else(|| NativeError::MissingSretSlot)?;
        let a0 = A0.hw as u32;
        let s0 = S0.hw as u32;
        // addi a0, s0, sret_offset (compute buffer address)
        self.push_u32(encode_addi(a0, s0, sret_offset as i32));
        
        // Step 3: Move user args to a1-a7 (shifted by 1)
        for (i, a) in args.iter().enumerate() {
            let from = self.use_vreg(alloc, *a, Self::TEMP0)? as u32;
            let to = ARG_REGS[i + 1].hw as u32;  // +1 because a0 is sret pointer
            if from != to {
                self.push_u32(encode_addi(to, from, 0));
            }
        }
        
        // Step 4: Emit auipc+jalr
        let auipc_off = self.code.len();
        let ra = abi::RA.hw as u32;
        self.push_u32(encode_auipc(ra, 0));
        self.push_u32(encode_jalr(ra, ra, 0));
        self.relocs.push(NativeReloc {
            offset: auipc_off,
            symbol: target.name.clone(),
        });
        
        // Step 5: Load return values from buffer into result vregs
        // Buffer is at fp + sret_offset, results are at buffer[0..rets.len()*4]
        let base_reg = s0;  // frame pointer
        for (i, r) in rets.iter().enumerate() {
            let offset = sret_offset + (i * 4) as i32;
            let dst = self.def_vreg(alloc, *r, Self::TEMP0)? as u32;
            self.push_u32(encode_lw(dst, base_reg, offset));
            self.store_def_vreg(alloc, *r, Self::TEMP0);
        }
        
        Ok(())
    }
}
```

**Add error variant:**

In `error.rs`, add:

```rust
#[derive(Debug)]
pub enum NativeError {
    // ... existing variants ...
    MissingSretSlot,  // NEW: tried to emit sret call but frame has no sret slot
}
```

**Update `emit_module_elf`:**

```rust
pub fn emit_module_elf(
    ir: &lpir::IrModule,
    sig: &lps_shared::LpsModuleSig,
    float_mode: lpir::FloatMode,
) -> Result<Vec<u8>, NativeError> {
    if ir.functions.is_empty() {
        return Err(NativeError::EmptyModule);
    }
    
    // NEW: Create ModuleAbi once for all functions
    let module_abi = ModuleAbi::from_lps_module_sig(sig);
    
    // ... rest of function, pass &module_abi to emit_function_bytes ...
    for func in &ir.functions {
        let emitted = emit_function_bytes(func, &module_abi, float_mode, false)?;
        // ... rest unchanged
    }
}
```

### Update EmitContext::new

```rust
pub fn new(is_leaf: bool, debug_info: bool) -> Self {
    let sig = lps_shared::LpsFnSig {
        name: String::from("__leaf"),
        return_type: lps_shared::LpsType::Void,
        parameters: vec![],
    };
    let func_abi = func_abi_rv32(&sig, 1);
    let frame = FrameLayout::compute(&func_abi, 0, PregSet::EMPTY, &[], is_leaf, 0);  // ADD 0
    Self::with_frame(frame, debug_info)
}
```

### Tests to Add

```rust
#[test]
fn emit_call_direct_basic() {
    let alloc = Allocation {
        vreg_to_phys: vec![Some(10), Some(11), Some(12)],  // a0, a1, a2
        spill_slots: vec![],
    };
    
    let mut ctx = EmitContext::new(false, false);
    ctx.emit_call_direct(
        &alloc,
        &SymbolRef { name: String::from("helper") },
        &[VReg(0), VReg(1)],  // args in v0, v1 -> a0, a1
        &[VReg(2)],           // result in v2 <- a0
    ).expect("emit");
    
    // Should have: (no moves, already in right regs) + auipc + jalr + (no move for result)
    // = 2 instructions
    assert_eq!(ctx.code.len(), 8);  // 2 * 4 bytes
    assert_eq!(ctx.relocs.len(), 1);
}

#[test]
fn emit_call_sret_basic() {
    use crate::abi::frame::FrameLayout;
    use crate::abi::{PReg, PregSet, RegClass};
    
    // Create frame with sret slot
    let func_abi = func_abi_rv32(&lps_shared::LpsFnSig {
        name: String::from("caller"),
        return_type: lps_shared::LpsType::Void,
        parameters: vec![],
    }, 1);
    
    let frame = FrameLayout::compute(&func_abi, 0, PregSet::EMPTY, &[], false, 16);
    let mut ctx = EmitContext::with_frame(frame, false);
    
    let alloc = Allocation {
        vreg_to_phys: vec![Some(10), Some(11), Some(12), Some(13), Some(14)],  // a0-a4
        spill_slots: vec![],
    };
    
    ctx.emit_call_sret(
        &alloc,
        &SymbolRef { name: String::from("returns_vec4") },
        &[VReg(1)],          // one arg (vmctx)
        &[VReg(2), VReg(3), VReg(4), VReg(5)],  // vec4 = 4 results
    ).expect("emit");
    
    // Should have: addi (buffer addr) + (no arg move) + auipc + jalr + 4*lw
    assert!(ctx.code.len() > 8);
    assert_eq!(ctx.relocs.len(), 1);
}
```

## Validate

```bash
cargo test -p lpvm-native emit_call
cargo check -p lpvm-native
```

Ensure:
- Tests pass
- No compiler warnings
- All code compiles
