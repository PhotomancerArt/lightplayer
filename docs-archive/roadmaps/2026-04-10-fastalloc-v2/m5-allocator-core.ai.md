# M5: Allocator Core

## Scope of Work

Implement the actual register allocation decisions: free register pool, LRU eviction, spill slot assignment, and reload generation.

## Files

```
lp-shader/lpvm-native/src/isa/rv32fa/alloc/
├── walk.rs                  # UPDATE: replace stubs with real allocation
└── spill.rs                 # NEW: spill slot management
```

## Implementation Details

### 1. Spill Slot Management in `spill.rs`

```rust
//! Spill slot allocation and tracking.
//!
//! Memory-efficient: uses i8 with -1 sentinel instead of Option<u32>.

use alloc::vec::Vec;

/// Spill slot allocator.
pub struct SpillAlloc {
    /// Spill slot for each vreg (-1 = not spilled, 0+ = slot index).
    /// Using i8 saves 3 bytes per vreg vs Option<u32>.
    vreg_spill: Vec<i8>,
    /// Next available spill slot.
    next_slot: u8,
}

impl SpillAlloc {
    pub fn new(num_vregs: usize) -> Self {
        Self {
            vreg_spill: vec![-1; num_vregs],
            next_slot: 0,
        }
    }

    /// Get or assign spill slot for vreg.
    pub fn get_or_assign(&mut self, vreg: VReg) -> u8 {
        let idx = vreg.0 as usize;
        let slot = self.vreg_spill[idx];
        if slot >= 0 {
            slot as u8
        } else {
            let new_slot = self.next_slot;
            self.vreg_spill[idx] = new_slot as i8;
            self.next_slot += 1;
            new_slot
        }
    }

    /// Check if vreg has a spill slot.
    pub fn has_slot(&self, vreg: VReg) -> Option<u8> {
        let slot = self.vreg_spill[vreg.0 as usize];
        if slot >= 0 {
            Some(slot as u8)
        } else {
            None
        }
    }

    /// Total spill slots used.
    pub fn total_slots(&self) -> u8 {
        self.next_slot
    }
}
```

### 2. Register Pool in `walk.rs`

```rust
//! Backward walk allocator with real allocation decisions.

use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use crate::error::NativeError;
use crate::vinst::VReg;
use crate::isa::rv32fa::abi::{allocatable_int, reg_name};

/// Register pool with LRU eviction.
pub struct RegPool {
    /// Which vreg is in each preg (None = free).
    preg_vreg: [Option<VReg>; 32],
    /// LRU order: most recent at end.
    lru: Vec<u8>,
}

impl RegPool {
    pub fn new() -> Self {
        let mut lru = Vec::new();
        for preg in allocatable_int().iter() {
            lru.push(preg.hw);
        }

        Self {
            preg_vreg: [None; 32],
            lru,
        }
    }

    /// Find free register or evict LRU.
    pub fn alloc(&mut self, want: VReg) -> Result<u8, NativeError> {
        // Find first free preg
        for (i, v) in self.preg_vreg.iter().enumerate() {
            if v.is_none() && self.is_allocatable(i as u8) {
                self.preg_vreg[i] = Some(want);
                self.touch(i as u8);
                return Ok(i as u8);
            }
        }

        // Evict LRU
        let victim = self.lru.remove(0);
        self.preg_vreg[victim as usize] = Some(want);
        self.touch(victim);
        Ok(victim)
    }

    /// Mark preg as most recently used.
    pub fn touch(&mut self, preg: u8) {
        if let Some(pos) = self.lru.iter().position(|&p| p == preg) {
            let p = self.lru.remove(pos);
            self.lru.push(p);
        }
    }

    /// Free a register.
    pub fn free(&mut self, preg: u8) {
        self.preg_vreg[preg as usize] = None;
    }

    /// Check if preg is allocatable.
    fn is_allocatable(&self, preg: u8) -> bool {
        // Not x0, ra, sp, gp, tp, s1 (reserved for SRET)
        ![0, 1, 2, 3, 4, 9].contains(&preg)
    }

    /// Get current preg for vreg if assigned.
    pub fn vreg_home(&self, vreg: VReg) -> Option<u8> {
        for (i, v) in self.preg_vreg.iter().enumerate() {
            if *v == Some(vreg) {
                return Some(i as u8);
            }
        }
        None
    }
}
```

### 3. Full Walk State

```rust
pub struct WalkState {
    reg_pool: RegPool,
    spill: SpillAlloc,
    trace: AllocTrace,
    /// Current live set (vregs that have been seen as uses but not yet killed by def).
    live: BTreeSet<VReg>,
}

impl WalkState {
    pub fn new(num_vregs: usize) -> Self {
        Self {
            reg_pool: RegPool::new(),
            spill: SpillAlloc::new(num_vregs),
            trace: AllocTrace::new(),
            live: BTreeSet::new(),
        }
    }

    fn process_instruction(
        &mut self,
        pos: usize,
        vinst: &VInst,
    ) -> Result<TraceEntry, NativeError> {
        let mut physinsts = Vec::new();
        let mut decision = String::new();

        // 1. Handle defs (late): values are "killed" here, freeing their registers
        for def in vinst.defs() {
            if let Some(preg) = self.reg_pool.vreg_home(def) {
                // If this def overwrites a live value, we need to spill it first
                if self.live.contains(&def) {
                    let slot = self.spill.get_or_assign(def);
                    physinsts.push(PhysInst::Store32 {
                        src: preg,
                        base: FP_REG,
                        offset: -((slot + 1) as i32 * 4),
                    });
                    decision.push_str(&format!(" spill v{} to [fp-{}]", def.0, (slot+1)*4));
                }
                self.reg_pool.free(preg);
            }
            self.live.remove(&def);
        }

        // 2. Handle uses (early): ensure values are in registers
        let mut use_preloads = Vec::new();
        let mut use_regs = Vec::new();

        for use_vreg in vinst.uses() {
            let preg = self.ensure_in_reg(use_vreg, &mut use_preloads)?;
            use_regs.push(preg);
            self.live.insert(use_vreg);
            self.reg_pool.touch(preg);
        }

        // 3. Emit the main instruction with resolved registers
        let main_inst = self.emit_vinst(vinst, &use_regs)?;
        physinsts.extend(use_preloads);
        physinsts.push(main_inst);

        // 4. Record decision
        for (i, use_vreg) in vinst.uses().enumerate() {
            if i > 0 { decision.push_str(", "); }
            decision.push_str(&format!("v{}->{}", use_vreg.0, reg_name(use_regs[i])));
        }

        Ok(TraceEntry::new(pos, vinst.src_op(), vinst.clone(), decision, physinsts))
    }

    fn ensure_in_reg(
        &mut self,
        vreg: VReg,
        reloads: &mut Vec<PhysInst>,
    ) -> Result<u8, NativeError> {
        // Check if already in a register
        if let Some(preg) = self.reg_pool.vreg_home(vreg) {
            return Ok(preg);
        }

        // Check if spilled
        if let Some(slot) = self.spill.has_slot(vreg) {
            // Reload into free register
            let preg = self.reg_pool.alloc(vreg)?;
            reloads.push(PhysInst::Load32 {
                dst: preg,
                base: FP_REG,
                offset: -((slot + 1) as i32 * 4),
            });
            return Ok(preg);
        }

        // Must be IConst32 - materialize immediate
        // TODO: Need to track the constant value somewhere
        let preg = self.reg_pool.alloc(vreg)?;
        // reloads.push(PhysInst::LoadImm { dst: preg, val: ??? });
        Ok(preg)
    }

    fn emit_vinst(&self, vinst: &VInst, use_regs: &[u8]) -> Result<PhysInst, NativeError> {
        match vinst {
            VInst::Add32 { dst, .. } => {
                let dst_preg = self.reg_pool.alloc(*dst)?;
                Ok(PhysInst::Add32 {
                    dst: dst_preg,
                    src1: use_regs[0],
                    src2: use_regs[1],
                })
            }
            // ... all other variants
        }
    }
}
```

### 4. IConst32 Handling

Need to track IConst32 values for rematerialization:

```rust
/// Tracks IConst32 values for rematerialization.
pub struct ConstPool {
    vreg_const: Vec<Option<i32>>,
}

impl ConstPool {
    pub fn new(num_vregs: usize) -> Self {
        Self { vreg_const: vec![None; num_vregs] }
    }

    pub fn set(&mut self, vreg: VReg, val: i32) {
        self.vreg_const[vreg.0 as usize] = Some(val);
    }

    pub fn get(&self, vreg: VReg) -> Option<i32> {
        self.vreg_const[vreg.0 as usize]
    }
}
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::debug::vinst::parse_vinsts;

    #[test]
    fn test_alloc_simple_add() {
        let vinsts = parse_vinsts("
            v0 = IConst32 1
            v1 = IConst32 2
            v2 = Add32 v0, v1
            Ret v2
        ").unwrap();

        let mut state = WalkState::new(3);
        // ... walk and allocate ...

        // Check trace shows register assignments
        let entry = &state.trace.entries[2];  // Add32 entry
        assert!(entry.decision.contains("v0->"));
        assert!(entry.decision.contains("v1->"));
    }

    #[test]
    fn test_spill_when_pressure_high() {
        // Create a test with more live values than registers
        // Should see spill/reload in trace
    }
}
```

## Validate

```bash
cd lp-shader/lpvm-native
cargo test -p lpvm-native --lib -- rv32fa::alloc::walk
```

## Success Criteria

1. Simple cases (iconst + add) produce correct PhysInsts
2. Trace shows register assignments for each VInst
3. Spill slots are assigned when register pressure is high
4. Reloads are generated before uses of spilled values
5. IConst32 values are rematerialized (not stored in registers)
