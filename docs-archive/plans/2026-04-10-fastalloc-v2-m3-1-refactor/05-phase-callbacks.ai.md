# Phase 5: Replace defs()/uses() with Callback API

## Scope

Replace the allocating `defs()` and `uses()` methods with zero-allocation callback-based `for_each_def()` and `for_each_use()`. These need access to the vreg_pool to resolve VRegSlice contents.

## Implementation

### 1. Update `vinst.rs` - Remove old methods

Remove or comment out the old methods:

```rust
// OLD - remove these:
// pub fn defs(&self) -> impl Iterator<Item = VReg> + '_
// pub fn uses(&self) -> impl Iterator<Item = VReg> + '_
```

### 2. Add new callback methods

```rust
impl VInst {
    /// Visit each vreg defined by this instruction.
    /// Callback receives the vreg directly.
    /// Zero allocation - just stack frames.
    pub fn for_each_def<F: FnMut(VReg)>(&self, pool: &[VReg], mut f: F) {
        match self {
            // Single def variants
            VInst::Add32 { dst, .. }
            | VInst::Sub32 { dst, .. }
            | VInst::Neg32 { dst, .. }
            | VInst::Mul32 { dst, .. }
            | VInst::And32 { dst, .. }
            | VInst::Or32 { dst, .. }
            | VInst::Xor32 { dst, .. }
            | VInst::Bnot32 { dst, .. }
            | VInst::Shl32 { dst, .. }
            | VInst::ShrS32 { dst, .. }
            | VInst::ShrU32 { dst, .. }
            | VInst::DivS32 { dst, .. }
            | VInst::DivU32 { dst, .. }
            | VInst::RemS32 { dst, .. }
            | VInst::RemU32 { dst, .. }
            | VInst::Icmp32 { dst, .. }
            | VInst::IeqImm32 { dst, .. }
            | VInst::Select32 { dst, .. }
            | VInst::Mov32 { dst, .. }
            | VInst::Load32 { dst, .. }
            | VInst::SlotAddr { dst, .. }
            | VInst::IConst32 { dst, .. } => f(*dst),

            // Multi-def: Call
            VInst::Call { rets, .. } => {
                for vreg in rets.iter(pool) {
                    f(vreg);
                }
            }

            // No def variants
            VInst::Store32 { .. }
            | VInst::MemcpyWords { .. }
            | VInst::Label(..)
            | VInst::Br { .. }
            | VInst::BrIf { .. }
            | VInst::Ret { .. } => {}
        }
    }

    /// Visit each vreg used by this instruction.
    /// Callback receives the vreg directly.
    /// Zero allocation - just stack frames.
    pub fn for_each_use<F: FnMut(VReg)>(&self, pool: &[VReg], mut f: F) {
        match self {
            // Two use variants
            VInst::Add32 { src1, src2, .. }
            | VInst::Sub32 { src1, src2, .. }
            | VInst::Mul32 { src1, src2, .. }
            | VInst::And32 { src1, src2, .. }
            | VInst::Or32 { src1, src2, .. }
            | VInst::Xor32 { src1, src2, .. }
            | VInst::Shl32 { src1, src2, .. }
            | VInst::ShrS32 { src1, src2, .. }
            | VInst::ShrU32 { src1, src2, .. } => {
                f(*src1);
                f(*src2);
            }

            // Div/rem variants (different field names)
            VInst::DivS32 { lhs, rhs, .. }
            | VInst::DivU32 { lhs, rhs, .. }
            | VInst::RemS32 { lhs, rhs, .. }
            | VInst::RemU32 { lhs, rhs, .. }
            | VInst::Icmp32 { lhs, rhs, .. } => {
                f(*lhs);
                f(*rhs);
            }

            // Single use variants
            VInst::Neg32 { src, .. }
            | VInst::Bnot32 { src, .. }
            | VInst::Mov32 { src, .. }
            | VInst::IeqImm32 { src, .. }
            | VInst::BrIf { cond: src, .. } => f(*src),

            // Load/Store variants
            VInst::Load32 { base, .. } => f(*base),
            VInst::Store32 { src, base, .. } => {
                f(*src);
                f(*base);
            }

            // Select: 3 uses
            VInst::Select32 { cond, if_true, if_false, .. } => {
                f(*cond);
                f(*if_true);
                f(*if_false);
            }

            // Memcpy: 2 uses
            VInst::MemcpyWords { dst_base, src_base, .. } => {
                f(*dst_base);
                f(*src_base);
            }

            // SlotAddr: no uses (just computes address)
            VInst::SlotAddr { .. } => {}

            // IConst: no uses
            VInst::IConst32 { .. } => {}

            // Call: multi-use via slice
            VInst::Call { args, .. } => {
                for vreg in args.iter(pool) {
                    f(vreg);
                }
            }

            // Ret: multi-use via slice
            VInst::Ret { vals, .. } => {
                for vreg in vals.iter(pool) {
                    f(vreg);
                }
            }

            // No use variants
            VInst::Label(..)
            | VInst::Br { .. } => {}
        }
    }

    /// Count uses (convenience for allocation heuristics).
    /// Zero allocation.
    pub fn use_count(&self, pool: &[VReg]) -> usize {
        let mut count = 0;
        self.for_each_use(pool, |_| count += 1);
        count
    }

    /// Count defs (convenience for allocation heuristics).
    /// Zero allocation.
    pub fn def_count(&self, pool: &[VReg]) -> usize {
        let mut count = 0;
        self.for_each_def(pool, |_| count += 1);
        count
    }
}
```

### 3. Add compatibility helper for tests/debug

```rust
/// Collect defs into a Vec for tests/debugging.
/// Allocates - only use in non-hot paths!
pub fn defs_to_vec(&self, pool: &[VReg]) -> Vec<VReg> {
    let mut result = Vec::new();
    self.for_each_def(pool, |v| result.push(v));
    result
}

/// Collect uses into a Vec for tests/debugging.
/// Allocates - only use in non-hot paths!
pub fn uses_to_vec(&self, pool: &[VReg]) -> Vec<VReg> {
    let mut result = Vec::new();
    self.for_each_use(pool, |v| result.push(v));
    result
}
```

### 4. Add unit tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> Vec<VReg> {
        vec![VReg(0), VReg(1), VReg(2), VReg(3)]
    }

    #[test]
    fn test_for_each_def_add32() {
        let inst = VInst::Add32 {
            dst: VReg(5),
            src1: VReg(0),
            src2: VReg(1),
            src_op: SRC_OP_NONE,
        };
        
        let mut defs = Vec::new();
        inst.for_each_def(&[], |v| defs.push(v));
        assert_eq!(defs, vec![VReg(5)]);
    }

    #[test]
    fn test_for_each_use_add32() {
        let inst = VInst::Add32 {
            dst: VReg(5),
            src1: VReg(0),
            src2: VReg(1),
            src_op: SRC_OP_NONE,
        };
        
        let mut uses = Vec::new();
        inst.for_each_use(&[], |v| uses.push(v));
        assert_eq!(uses, vec![VReg(0), VReg(1)]);
    }

    #[test]
    fn test_for_each_def_call() {
        let pool = test_pool();
        // Call with rets slice pointing to pool[2..3]
        let inst = VInst::Call {
            target: SymbolId(0),
            args: VRegSlice::new(0, 2),  // pool[0..2]
            rets: VRegSlice::new(2, 1),  // pool[2..3]
            callee_uses_sret: false,
            src_op: SRC_OP_NONE,
        };
        
        let mut defs = Vec::new();
        inst.for_each_def(&pool, |v| defs.push(v));
        assert_eq!(defs, vec![VReg(2)]);
    }

    #[test]
    fn test_for_each_use_call() {
        let pool = test_pool();
        let inst = VInst::Call {
            target: SymbolId(0),
            args: VRegSlice::new(0, 2),  // pool[0..2]
            rets: VRegSlice::new(2, 1),
            callee_uses_sret: false,
            src_op: SRC_OP_NONE,
        };
        
        let mut uses = Vec::new();
        inst.for_each_use(&pool, |v| uses.push(v));
        assert_eq!(uses, vec![VReg(0), VReg(1)]);
    }

    #[test]
    fn test_for_each_def_ret() {
        let pool = test_pool();
        let inst = VInst::Ret {
            vals: VRegSlice::new(0, 2),  // pool[0..2]
            src_op: SRC_OP_NONE,
        };
        
        // Ret has no defs
        let mut defs = Vec::new();
        inst.for_each_def(&pool, |v| defs.push(v));
        assert!(defs.is_empty());
        
        // Ret has uses
        let mut uses = Vec::new();
        inst.for_each_use(&pool, |v| uses.push(v));
        assert_eq!(uses, vec![VReg(0), VReg(1)]);
    }
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- vinst::tests
```

All tests should pass.
