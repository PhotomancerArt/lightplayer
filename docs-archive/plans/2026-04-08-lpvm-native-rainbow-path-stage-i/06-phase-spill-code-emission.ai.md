## Scope of Phase

Implement spill code emission (LoadSpill, StoreSpill VInsts) with s0-relative addressing.

## Code Organization Reminders

- Add `LoadSpill` and `StoreSpill` to `VInst` enum
- Emit `lw`/`sw` with s0-relative negative offsets
- Handle spill slot to offset conversion

## Implementation Details

### VInst additions

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VInst {
    // ... existing variants ...
    
    /// Load from spill slot: dst = *(s0 + offset)
    LoadSpill {
        dst: VReg,
        slot: u32,  // spill slot index
        src_op: Option<u32>,
    },
    
    /// Store to spill slot: *(s0 + offset) = src
    StoreSpill {
        src: VReg,
        slot: u32,
        src_op: Option<u32>,
    },
}

impl VInst {
    pub fn is_spill(&self) -> bool {
        matches!(self, VInst::LoadSpill { .. } | VInst::StoreSpill { .. })
    }
}
```

### Emit spill code

```rust
impl EmitContext {
    fn spill_to_offset(&self, slot: u32) -> i32 {
        self.frame_layout.spill_to_offset(slot)
    }
    
    pub fn emit_vinst(&mut self, inst: &VInst, alloc: &Allocation) -> Result<(), NativeError> {
        // ... existing cases ...
        
        match inst {
            // ... existing ...
            
            VInst::LoadSpill { dst, slot, .. } => {
                let rd = Self::phys(alloc, *dst)? as u32;
                let s0 = S0 as u32;
                let offset = self.spill_to_offset(*slot);
                self.push_u32(encode_lw(rd, s0, offset));
            }
            
            VInst::StoreSpill { src, slot, .. } => {
                let rs = Self::phys(alloc, *src)? as u32;
                let s0 = S0 as u32;
                let offset = self.spill_to_offset(*slot);
                self.push_u32(encode_sw(rs, s0, offset));
            }
            
            // ... rest ...
        }
        Ok(())
    }
}
```

### Error handling for spilled vregs

Update `phys()` to handle spilled vregs:

```rust
fn phys(alloc: &Allocation, v: VReg) -> Result<PhysReg, NativeError> {
    let i = v.0 as usize;
    alloc
        .vreg_to_phys
        .get(i)
        .copied()
        .flatten()
        .ok_or_else(|| {
            if alloc.is_spilled(v) {
                NativeError::SpilledVReg(v.0)
            } else {
                NativeError::UnassignedVReg(v.0)
            }
        })
}
```

## Tests to Write

```rust
#[test]
fn emit_load_spill() {
    let mut ctx = EmitContext::new(test_frame(1), true, false);
    ctx.emit_vinst(&VInst::LoadSpill {
        dst: VReg(0),
        slot: 0,
        src_op: None,
    }, &test_alloc()).expect("emit");
    
    // Should emit: lw x0, -8(s0)
    assert_eq!(ctx.code.len(), 4);
}

#[test]
fn emit_store_spill() {
    let mut ctx = EmitContext::new(test_frame(1), true, false);
    ctx.emit_vinst(&VInst::StoreSpill {
        src: VReg(0),
        slot: 0,
        src_op: None,
    }, &test_alloc()).expect("emit");
    
    // Should emit: sw x0, -8(s0)
    assert_eq!(ctx.code.len(), 4);
}

#[test]
fn spill_offsets_in_range() {
    // Verify offsets fit in 12-bit immediate
    for slot in 0..100 {
        let offset = -((8 + slot * 4) as i32);
        assert!(offset >= -2048 && offset <= 2047);
    }
}
```

## Validate

```bash
cargo test -p lpvm-native emit_spill
cargo test -p lpvm-native spill_offset
```
