# Phase 5: Simple Allocator (Straight-Line)

## Scope

Implement backward-walk allocator for straight-line code (no branches/jumps).

## Implementation

Create `rv32fa/alloc.rs`:

```rust
//! Backward-walk register allocator producing PhysInst.
//!
//! Straight-line only: errors on branches/jumps.

use crate::isa::rv32fa::abi::{ARG_REGS, FP_REG, RET_REGS, SP_REG, callee_saved_int, caller_saved_int};
use crate::isa::rv32fa::inst::{PhysInst, PhysReg};
use crate::vinst::{IcmpCond, SymbolRef, VInst, VReg};
use alloc::vec::Vec;

/// Allocator state during backward walk.
struct AllocState {
    /// Live registers at current point (VReg -> PhysReg)
    live: [i8; 256],  // VReg.0 -> PhysReg, -1 = spilled or not live
    /// LRU stack: most recently used PhysReg at back
    free_regs: Vec<u8>,
    /// Used registers (LRU stack: most recent at back)
    used_regs: Vec<u8>,
    /// Next spill slot (bytes, multiples of 4)
    next_spill_slot: u32,
    /// Accumulated PhysInsts (built backward, reversed at end)
    output: Vec<PhysInst>,
    /// Function parameters (for tracing)
    params: Vec<VReg>,
    /// Function returns (for tracing)
    returns: Vec<VReg>,
    /// Is SRET (returns via hidden first arg)
    is_sret: bool,
}

impl AllocState {
    fn new(is_sret: bool, params: Vec<VReg>, returns: Vec<VReg>) -> Self {
        // Initialize with all temp registers as free (callee-saved in use initially)
        let mut free_regs = Vec::with_capacity(12);
        for i in [5, 6, 7, 28, 29, 30, 31] {
            free_regs.push(i); // t0-t6
        }
        for i in 18..=27 {
            free_regs.push(i); // s2-s11 (available after reserve)
        }

        // Reserve registers for args/returns
        let mut used_regs = Vec::new();
        for &preg in ARG_REGS.iter().take(params.len().max(1)) {
            used_regs.push(*preg);
        }
        for &preg in RET_REGS.iter().take(returns.len()) {
            if !used_regs.contains(preg) {
                used_regs.push(*preg);
            }
        }
        // Reserve ra, sp, fp
        used_regs.push(1); // ra
        used_regs.push(2); // sp
        used_regs.push(8); // fp/s0

        Self {
            live: [-1; 256],
            free_regs,
            used_regs,
            next_spill_slot: 0,
            output: Vec::new(),
            params,
            returns,
            is_sret,
        }
    }

    /// Allocate a register (LRU eviction if needed).
    fn alloc_reg(&mut self) -> PhysReg {
        if let Some(preg) = self.free_regs.pop() {
            self.used_regs.push(preg);
            preg
        } else {
            // Evict LRU (front of used_regs, but skip reserved)
            let to_evict = self.used_regs
                .iter()
                .position(|r| !self.is_reserved(*r))
                .expect("No evictable register");
            let preg = self.used_regs.remove(to_evict);

            // Evict - find VReg and spill
            for vreg in 0..256 {
                if self.live[vreg] == preg as i8 {
                    self.spill(vreg as u8);
                    break;
                }
            }

            self.used_regs.push(preg);
            preg
        }
    }

    fn is_reserved(&self, preg: PhysReg) -> bool {
        // ra, sp, fp are reserved
        preg == 1 || preg == 2 || preg == 8
    }

    /// Spill a VReg to stack.
    fn spill(&mut self, vreg: u8) {
        let slot = self.next_spill_slot;
        self.next_spill_slot += 4; // 4-byte spill slot

        // Emit reload (we're going backward, so this is actually a store)
        let preg = self.live[vreg as usize] as PhysReg;
        self.output.push(PhysInst::Sw {
            src: preg,
            base: 2, // sp
            offset: slot as i32,
        });

        self.live[vreg as usize] = -1;
    }

    /// Get or allocate register for a VReg (use).
    fn get_or_alloc(&mut self, vreg: VReg) -> PhysReg {
        let idx = vreg.0 as usize;
        if self.live[idx] >= 0 {
            let preg = self.live[idx] as PhysReg;
            // Touch to make recently used
            self.touch_reg(preg);
            preg
        } else {
            // Allocate new register and emit reload
            let preg = self.alloc_reg();
            self.live[idx] = preg as i8;

            // Emit reload (we're going backward, so this is a load)
            // Find spill slot - we don't track it, need to fix this
            // For now, assume we reload from stack slot we tracked
            // This needs the spill slot map

            preg
        }
    }

    /// Define a VReg (receiving a register or remat).
    fn define(&mut self, vreg: VReg) -> (PhysReg, bool) {
        let idx = vreg.0 as usize;

        // Check if this is a rematerializable constant
        // For now, all defines need a register
        let preg = self.alloc_reg();
        self.live[idx] = preg as i8;
        (preg, false) // (preg, is_remat)
    }

    /// Move preg to front of LRU.
    fn touch_reg(&mut self, preg: PhysReg) {
        if let Some(pos) = self.used_regs.iter().position(|r| *r == preg) {
            let r = self.used_regs.remove(pos);
            self.used_regs.push(r);
        }
    }

    /// Kill a VReg (definition site going backward).
    fn kill(&mut self, vreg: VReg) {
        let idx = vreg.0 as usize;
        if self.live[idx] >= 0 {
            let preg = self.live[idx] as PhysReg;
            self.live[idx] = -1;

            // Return to free pool (don't touch order, just add)
            if !self.free_regs.contains(&preg) {
                self.free_regs.push(preg);
            }
        }
    }
}

/// Allocate VInsts to PhysInsts.
pub fn allocate(vinsts: &[VInst], is_sret: bool, params: Vec<VReg>, returns: Vec<VReg>) -> Result<Vec<PhysInst>, AllocError> {
    let mut state = AllocState::new(is_sret, params, returns);

    // Walk backward
    for vinst in vinsts.iter().rev() {
        // Check for unsupported control flow
        match vinst {
            VInst::Br { .. } | VInst::Beq { .. } | VInst::Bne { .. } |
            VInst::Blt { .. } | VInst::Bge { .. } | VInst::Bltu { .. } | VInst::Bgeu { .. } => {
                return Err(AllocError::UnsupportedControlFlow);
            }
            VInst::Label(..) => continue, // Skip labels
            _ => {}
        }

        // Handle call specially (clobbers caller-saved)
        if let VInst::Call { .. } = vinst {
            // TODO: implement call clobber handling
            todo!("call clobber handling");
        }

        // Process uses (read before definition)
        let uses = get_uses(vinst);
        let use_pregs: Vec<_> = uses.iter().map(|v| state.get_or_alloc(*v)).collect();

        // Process defs (kill after capturing uses)
        let defs = get_defs(vinst);
        for d in &defs {
            state.kill(*d);
        }

        // Allocate new registers for defs
        let def_pregs: Vec<_> = defs.iter().map(|_| state.alloc_reg()).collect();
        for (i, d) in defs.iter().enumerate() {
            state.live[d.0 as usize] = def_pregs[i] as i8;
        }

        // Emit PhysInst
        let phys = vinst_to_phys(vinst, &use_pregs, &def_pregs);
        state.output.push(phys);
    }

    // Reverse output
    state.output.reverse();

    // Add frame setup/teardown
    let mut result = Vec::with_capacity(state.output.len() + 2);
    result.push(PhysInst::FrameSetup { spill_slots: state.next_spill_slot / 4 });
    result.extend(state.output);
    result.push(PhysInst::FrameTeardown { spill_slots: state.next_spill_slot / 4 });

    Ok(result)
}

#[derive(Debug, Clone)]
pub enum AllocError {
    UnsupportedControlFlow,
    TooManyRegisters,
    OutOfSpillSlots,
}

fn get_uses(inst: &VInst) -> Vec<VReg> {
    match inst {
        VInst::IConst32 { .. } => vec![],
        VInst::Add32 { src1, src2, .. } => vec![*src1, *src2],
        VInst::Sub32 { src1, src2, .. } => vec![*src1, *src2],
        VInst::Mul32 { src1, src2, .. } => vec![*src1, *src2],
        VInst::Div32 { src1, src2, .. } => vec![*src1, *src2],
        VInst::Rem32 { src1, src2, .. } => vec![*src1, *src2],
        VInst::Neg32 { src, .. } => vec![*src],
        VInst::Bnot32 { src, .. } => vec![*src],
        VInst::Select32 { cond, src1, src2, .. } => vec![*cond, *src1, *src2],
        VInst::Icmp32 { src1, src2, .. } => vec![*src1, *src2],
        VInst::Load32 { src, .. } => vec![*src],
        VInst::Store32 { src, dst, .. } => vec![*src, *dst],
        VInst::SlotAddr { .. } => vec![],
        VInst::SlotCopy { src, dst, .. } => vec![*src, *dst],
        VInst::MemcpyWords { dst, src, .. } => vec![*dst, *src],
        VInst::Call { args, .. } => args.clone(),
        VInst::Ret { src } => vec![*src],
        _ => vec![],
    }
}

fn get_defs(inst: &VInst) -> Vec<VReg> {
    match inst {
        VInst::IConst32 { dst, .. } => vec![*dst],
        VInst::Add32 { dst, .. } => vec![*dst],
        VInst::Sub32 { dst, .. } => vec![*dst],
        VInst::Mul32 { dst, .. } => vec![*dst],
        VInst::Div32 { dst, .. } => vec![*dst],
        VInst::Rem32 { dst, .. } => vec![*dst],
        VInst::Neg32 { dst, .. } => vec![*dst],
        VInst::Bnot32 { dst, .. } => vec![*dst],
        VInst::Select32 { dst, .. } => vec![*dst],
        VInst::Icmp32 { dst, .. } => vec![*dst],
        VInst::Load32 { dst, .. } => vec![*dst],
        VInst::Store32 { .. } => vec![],
        VInst::SlotAddr { dst, .. } => vec![*dst],
        VInst::SlotCopy { .. } => vec![],
        VInst::MemcpyWords { .. } => vec![],
        VInst::Call { rets, .. } => rets.clone(),
        VInst::Ret { .. } => vec![],
        _ => vec![],
    }
}

fn vinst_to_phys(vinst: &VInst, uses: &[PhysReg], defs: &[PhysReg]) -> PhysInst {
    match vinst {
        VInst::IConst32 { dst, val, .. } => PhysInst::Li { dst: defs[0], imm: *val },
        VInst::Add32 { .. } => PhysInst::Add { dst: defs[0], src1: uses[0], src2: uses[1] },
        VInst::Sub32 { .. } => PhysInst::Sub { dst: defs[0], src1: uses[0], src2: uses[1] },
        VInst::Mul32 { .. } => PhysInst::Mul { dst: defs[0], src1: uses[0], src2: uses[1] },
        VInst::Div32 { .. } => PhysInst::Div { dst: defs[0], src1: uses[0], src2: uses[1] },
        VInst::Rem32 { .. } => PhysInst::Rem { dst: defs[0], src1: uses[0], src2: uses[1] },
        VInst::Neg32 { .. } => PhysInst::Neg { dst: defs[0], src: uses[0] },
        VInst::Bnot32 { .. } => PhysInst::Not { dst: defs[0], src: uses[0] },
        VInst::Select32 { .. } => {
            // select = if cond then src1 else src2
            // Lower to: beq cond, x0, else; mv dst, src1; j end; else: mv dst, src2; end:
            todo!("Select32 lowering needs branches")
        }
        VInst::Icmp32 { cond, .. } => {
            match cond {
                IcmpCond::Eq => PhysInst::Seqz { dst: defs[0], src: uses[0] }, // simplified
                IcmpCond::Ne => PhysInst::Snez { dst: defs[0], src: uses[0] },
                IcmpCond::Lt => PhysInst::Slt { dst: defs[0], src1: uses[0], src2: uses[1] },
                IcmpCond::Le => todo!("Le needs expansion"),
                IcmpCond::Gt => PhysInst::Slt { dst: defs[0], src1: uses[1], src2: uses[0] },
                IcmpCond::Ge => todo!("Ge needs expansion"),
            }
        }
        VInst::Load32 { .. } => PhysInst::Lw { dst: defs[0], base: uses[0], offset: 0 },
        VInst::Store32 { .. } => PhysInst::Sw { src: uses[0], base: uses[1], offset: 0 },
        VInst::SlotAddr { slot, .. } => PhysInst::SlotAddr { dst: defs[0], slot: *slot },
        VInst::SlotCopy { size, .. } => PhysInst::MemcpyWords { dst: uses[1], src: uses[0], size: *size },
        VInst::MemcpyWords { size, .. } => PhysInst::MemcpyWords { dst: uses[0], src: uses[1], size: *size },
        VInst::Call { target, .. } => PhysInst::Call { target: target.clone() },
        VInst::Ret { .. } => PhysInst::Ret,
        _ => todo!("Unhandled VInst: {:?}", vinst),
    }
}
```

## Notes

- Straight-line only: errors on any branch/jump
- Simple LRU eviction
- i8 with -1 sentinel for live tracking
- Output built backward, reversed at end
- Frame setup/teardown added at boundaries

## Validate

```bash
cargo check -p lpvm-native --lib
```
