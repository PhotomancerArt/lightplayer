# Phase 2: Emission Context and Frame Layout

## Scope

Create `EmitContext` that accumulates code bytes and tracks state for frame layout. Implement prologue (stack setup, ra save) and epilogue (ra restore, stack teardown, ret).

## Code Organization

- EmitContext struct and methods first
- Prologue/epilogue helpers
- Tests at bottom

## Implementation Details

```rust
use alloc::vec::Vec;
use alloc::string::String;

/// Accumulates machine code bytes and relocation records.
pub struct EmitContext {
    /// Generated code bytes
    pub code: Vec<u8>,
    /// Relocations for external symbols
    pub relocs: Vec<NativeReloc>,
    /// Current SP offset from entry (grows negative)
    sp_offset: i32,
    /// Total frame size (positive, includes padding)
    frame_size: i32,
    /// Is this a leaf function (no calls)?
    is_leaf: bool,
}

/// Relocation record for external symbols.
pub struct NativeReloc {
    /// Byte offset where relocation applies (points to auipc in auipc+jalr pair)
    pub offset: usize,
    /// Symbol name (e.g., "__lpir_fadd_q32")
    pub symbol: String,
    /// Relocation kind
    pub kind: RelocKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RelocKind {
    /// R_RISCV_CALL_PLT — covers auipc+jalr pair (8 bytes)
    CallPlt,
}

/// Physical register numbers (from abi.rs)
pub const RA: u32 = 1;
pub const SP: u32 = 2;

impl EmitContext {
    pub fn new(is_leaf: bool) -> Self {
        // Fixed 16-byte frame for M2:
        // [sp-16] saved ra (4 bytes)
        // [sp-12] padding/spill
        // [sp-8]  spill slot
        // [sp-4]  spill slot
        let frame_size = 16;
        
        Self {
            code: Vec::new(),
            relocs: Vec::new(),
            sp_offset: 0,
            frame_size,
            is_leaf,
        }
    }
    
    /// Emit prologue: adjust SP, save ra (if non-leaf)
    pub fn emit_prologue(&mut self) {
        // addi sp, sp, -frame_size
        let addi_sp = encode_addi(SP, SP, -self.frame_size);
        self.push32(addi_sp);
        
        if !self.is_leaf {
            // sw ra, (frame_size-4)(sp)  -- save at top of frame
            let offset = self.frame_size - 4;
            let sw_ra = encode_sw(RA, SP, offset);
            self.push32(sw_ra);
        }
    }
    
    /// Emit epilogue: restore ra (if non-leaf), adjust SP, ret
    pub fn emit_epilogue(&mut self) {
        if !self.is_leaf {
            // lw ra, (frame_size-4)(sp)
            let offset = self.frame_size - 4;
            let lw_ra = encode_lw(RA, SP, offset);
            self.push32(lw_ra);
        }
        
        // addi sp, sp, frame_size
        let addi_sp = encode_addi(SP, SP, self.frame_size);
        self.push32(addi_sp);
        
        // ret (jalr x0, x1, 0)
        let ret = encode_ret();
        self.push32(ret);
    }
    
    /// Emit auipc+jalr pair for symbol call, record relocation.
    pub fn emit_call(&mut self, symbol: &str) {
        // auipc ra, 0
        let auipc_offset = self.code.len();
        let auipc = encode_auipc(RA, 0);
        self.push32(auipc);
        
        // jalr ra, ra, 0
        let jalr = encode_jalr(RA, RA, 0);
        self.push32(jalr);
        
        // Record R_RISCV_CALL_PLT relocation at auipc
        self.relocs.push(NativeReloc {
            offset: auipc_offset,
            symbol: String::from(symbol),
            kind: RelocKind::CallPlt,
        });
    }
    
    /// Emit lw from sp-relative slot to register.
    pub fn emit_load_slot(&mut self, rd: u32, slot_offset: i32) {
        let lw = encode_lw(rd, SP, slot_offset);
        self.push32(lw);
    }
    
    /// Emit sw from register to sp-relative slot.
    pub fn emit_store_slot(&mut self, rs: u32, slot_offset: i32) {
        let sw = encode_sw(rs, SP, slot_offset);
        self.push32(sw);
    }
    
    fn push32(&mut self, insn: u32) {
        self.code.extend_from_slice(&insn.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_prologue_epilogue_leaf() {
        let mut ctx = EmitContext::new(/*is_leaf=*/ true);
        ctx.emit_prologue();
        ctx.emit_epilogue();
        
        // Leaf: no ra save/restore
        // addi sp, sp, -16
        // addi sp, sp, 16
        // ret
        assert_eq!(ctx.code.len(), 12); // 3 * 4 bytes
        assert!(ctx.relocs.is_empty());
    }
    
    #[test]
    fn test_prologue_epilogue_non_leaf() {
        let mut ctx = EmitContext::new(/*is_leaf=*/ false);
        ctx.emit_prologue();
        ctx.emit_epilogue();
        
        // Non-leaf: ra save/restore
        // addi sp, sp, -16
        // sw ra, 12(sp)
        // lw ra, 12(sp)
        // addi sp, sp, 16
        // ret
        assert_eq!(ctx.code.len(), 20); // 5 * 4 bytes
        assert!(ctx.relocs.is_empty());
    }
    
    #[test]
    fn test_emit_call_records_reloc() {
        let mut ctx = EmitContext::new(/*is_leaf=*/ false);
        ctx.emit_call("__lpir_fadd_q32");
        
        // auipc + jalr = 8 bytes
        assert_eq!(ctx.code.len(), 8);
        // One relocation recorded
        assert_eq!(ctx.relocs.len(), 1);
        assert_eq!(ctx.relocs[0].symbol, "__lpir_fadd_q32");
        assert_eq!(ctx.relocs[0].kind, RelocKind::CallPlt);
    }
}
```

## Key Points

- Fixed 16-byte frame size for M2 POC (simplifies addressing)
- Non-leaf functions save/restore `ra` at `[sp + 12]` (top of frame)
- `emit_call()` emits `auipc+jalr` and records `CallPlt` relocation
- Stack slots for spills use `emit_load_slot`/`emit_store_slot`

## Tests to Write

1. `test_prologue_epilogue_leaf` — No ra save, just SP adjust and ret
2. `test_prologue_epilogue_non_leaf` — Includes ra save/restore
3. `test_emit_call_records_reloc` — Verify relocation tracking
4. `test_spill_slots` — Load/store to stack offsets

## Validate

```bash
cargo test -p lpvm-native --lib emit::tests
cargo check -p lpvm-native
```
