# M2 Design: RV32 Emission (ELF Output)

## Scope

Implement RV32 instruction encoding and emission pipeline producing **ELF object files** as the primary output. The emulator consumes ELF for validation; JIT execution is deferred to a later milestone.

### In Scope
- R/I/S/B/U/J instruction encoding functions (adapted from Cranelift fork)
- `isa/rv32/inst.rs` — pure encoding functions, no state
- `isa/rv32/emit.rs` — `EmitContext` accumulating bytes and relocations
- Shader ABI frame layout: prologue (ra save, sp adjust), epilogue (restore, ret)
- Builtin call emission with `auipc+jalr` sequence
- **ELF relocations**: R_RISCV_CALL_PLT via `object` crate
- Stack slot management (emergency spill paths)
- Integration: `NativeEngine::compile()` produces ELF bytes

### Out of Scope (Deferred)
- JIT buffer output and runtime relocation patching
- Control flow (branches, jumps, loops)
- 64-bit operations (I64Stub remains stub/panic)
- RVC compressed instructions
- Actual execution (emulator/filetests validate in later milestone)

## File Structure

```
lp-shader/lpvm-native/
├── Cargo.toml                # UPDATE: Add object crate dependency
└── src/
    ├── lib.rs                    # UPDATE: Re-export new modules
    ├── types.rs                  # (existing - NativeType)
    ├── vinst.rs                  # (existing - VInst enum)
    ├── lower.rs                  # (existing - lower_op)
    ├── error.rs                  # (existing - LowerError, NativeError)
    ├── regalloc/
    │   ├── mod.rs                # (existing - RegAlloc trait)
    │   └── greedy.rs             # UPDATE: Add live value limit check
    ├── isa/
    │   ├── mod.rs                # UPDATE: CodeBlob, RelocKind
    │   └── rv32/
    │       ├── mod.rs            # (existing - re-exports)
    │       ├── abi.rs            # (existing - FrameLayout, register lists)
    │       ├── inst.rs           # NEW: Instruction encoding (R/I/S/B/U/J)
    │       └── emit.rs           # UPDATE: EmitContext, emit_vinst, ELF reloc
    ├── engine.rs                 # UPDATE: compile() produces ELF
    └── module.rs                 # (existing - NativeModule)

lp-shader/lpvm-native/tests/
└── emit_tests.rs             # NEW: Encoding unit tests, objdump round-trip
```

## Conceptual Architecture

```
┌────────────────────────────────────────────────────────────┐
│                     NativeEngine                           │
│  compile(ir_function) -> Result<Vec<u8>, NativeError>     │
└────────────────────────────────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        ▼                   ▼                   ▼
   ┌─────────┐        ┌──────────┐        ┌─────────────┐
   │ lowering│──────▶│ regalloc │──────▶│   emission  │
   │(VInsts) │        │(Greedy)  │        │(isa::rv32)  │
   └─────────┘        └──────────┘        └─────────────┘
                                                │
                    ┌───────────────────────────┼───────────┐
                    ▼                           ▼           ▼
              ┌──────────┐              ┌─────────────┐  ┌────────┐
              │ code     │              │ relocations │  │symbols│
              │ bytes    │              │ (offset,    │  │(names) │
              │ (u8 vec) │              │  symbol)    │  │        │
              └──────────┘              └─────────────┘  └────────┘
                    │                           │            │
                    └───────────────────────────┴────────────┘
                                        │
                                        ▼
                              ┌──────────────────┐
                              │  object::Object  │
                              │  (ELF container) │
                              │  - .text section │
                              │  - .symtab       │
                              │  - .rela.text    │
                              └──────────────────┘
                                        │
                                        ▼
                              ┌──────────────────┐
                              │   ELF bytes      │
                              │   (Vec<u8>)      │
                              └──────────────────┘
```

## Key Components

### 1. Instruction Encoding (`isa/rv32/inst.rs`)

Pure functions adapted from Cranelift fork (`encode.rs`), stripped to essentials:

```rust
/// R-type: | funct7 | rs2 | rs1 | funct3 | rd | opcode |
pub fn encode_r_type(opcode: u32, rd: u32, funct3: u32, rs1: u32, rs2: u32, funct7: u32) -> u32;
pub fn encode_i_type(opcode: u32, rd: u32, funct3: u32, rs1: u32, imm: i32) -> u32;
pub fn encode_s_type(opcode: u32, funct3: u32, rs1: u32, rs2: u32, imm: i32) -> u32;
pub fn encode_u_type(opcode: u32, rd: u32, imm: i32) -> u32;
pub fn encode_j_type(opcode: u32, rd: u32, imm: i32) -> u32;

// Convenience wrappers for common instructions
pub fn encode_add(rd: u32, rs1: u32, rs2: u32) -> u32;
pub fn encode_lw(rd: u32, rs1: u32, imm: i32) -> u32;
pub fn encode_sw(rs1: u32, rs2: u32, imm: i32) -> u32;
pub fn encode_auipc(rd: u32, imm: i32) -> u32;
pub fn encode_jalr(rd: u32, rs1: u32, imm: i32) -> u32;
pub fn encode_ret() -> u32;  // jalr x0, x1, 0
```

**Validation**: Unit tests with hardcoded expected values (e.g., `assert_eq!(encode_add(1, 2, 3), 0x003100b3)`).

### 2. Emission Context (`isa/rv32/emit.rs`)

```rust
pub struct EmitContext {
    /// Accumulated code bytes
    code: Vec<u8>,
    /// Relocations for external symbols
    relocs: Vec<NativeReloc>,
    /// Current stack frame offset (grows negative)
    sp_offset: i32,
    /// Frame layout info
    frame: FrameLayout,
}

pub struct NativeReloc {
    /// Byte offset in code where relocation applies
    pub offset: usize,
    /// Symbol name (e.g., "__lpir_fadd_q32")
    pub symbol: String,
    /// Relocation kind
    pub kind: RelocKind,
}

pub enum RelocKind {
    /// R_RISCV_CALL_PLT — covers auipc+jalr pair
    CallPlt,
}

impl EmitContext {
    pub fn emit_prologue(&mut self, is_leaf: bool);
    pub fn emit_epilogue(&mut self, is_leaf: bool);
    pub fn emit_vinst(&mut self, vinst: &VInst, alloc: &Allocation);
    
    /// Emit auipc+jalr sequence with placeholder, record CallPlt reloc
    pub fn emit_call(&mut self, symbol: &str);
    
    /// Generate final ELF object
    pub fn finish_elf(self, func_name: &str) -> Vec<u8>;
}
```

### 3. Relocation and ELF Generation

Uses `object` crate with `write_core` feature (no_std compatible):

```toml
[dependencies]
object = { version = "0.38", default-features = false, features = ["write_core"] }
```

**In finish_elf()**:
```rust
let mut obj = object::Object::new(BinaryFormat::Elf, Architecture::Riscv32, Endianness::Little);
let section = obj.add_section(vec![], b".text".to_vec(), SectionKind::Text);
let symbol = obj.add_symbol(Symbol {
    name: func_name.as_bytes().to_vec(),
    section: SymbolSection::Section(section),
    ..Default::default()
});
// Add R_RISCV_CALL_PLT relocation at auipc offset
obj.add_relocation(section, Relocation {
    offset: reloc.offset as u64,
    symbol,
    relocation: RelocationKind::Elf(elf::R_RISCV_CALL_PLT as u16),
    ..Default::default()
})?;
```

**Instruction sequence for call**:
```asm
auipc ra, 0       # R_RISCV_CALL_PLT covers this + next (8 bytes)
jalr  ra, ra, 0   # 
```

The R_RISCV_CALL_PLT relocation tells the linker to compute: `(symbol - auipc_addr)` and split into hi20/lo12 parts.

### 4. Frame Layout (from M1 abi.rs)

```rust
// Stack grows down. Fixed 16-byte frame for M2:
// [sp-16]  saved ra (4 bytes)
// [sp-12]  padding or spill slot
// [sp-8]   spill slot
// [sp-4]   spill slot
// sp -= 16 on entry, sp += 16 on exit
```

Non-leaf functions (call builtins): save/restore `ra`.
Emergency spills: use `sw`/`lw` with sp-relative offsets.

### 5. Greedy Allocator Update

```rust
// In greedy.rs: limit live values to available registers
const MAX_LIVE: usize = 24; // x8-x31 minus any reserved

pub fn allocate(&self, func: &IrFunction) -> Result<Vec<Allocation>, NativeError> {
    // ... existing logic ...
    if live_set.len() > MAX_LIVE {
        return Err(NativeError::TooManyLiveValues(live_set.len()));
    }
    // ...
}
```

### 6. NativeEngine::compile()

```rust
impl LpvmEngine for NativeEngine {
    fn compile(&self, ir: &IrFunction, opts: &NativeCompileOptions) -> Result<NativeModule, NativeError> {
        // 1. Lower to VInst
        let vinsts = lower::lower_ops(ir)?;
        
        // 2. Allocate registers
        let alloc = self.reg_alloc.allocate(ir)?;
        
        // 3. Emit code
        let mut ctx = EmitContext::new(FrameLayout::default());
        ctx.emit_prologue(/*is_leaf=*/ false);
        for (vinst, allocs) in vinsts.iter().zip(&alloc.per_op) {
            ctx.emit_vinst(vinst, allocs);
        }
        ctx.emit_epilogue(/*is_leaf=*/ false);
        
        // 4. Generate ELF
        let elf_bytes = ctx.finish_elf(&ir.name);
        
        Ok(NativeModule { elf: elf_bytes })
    }
}
```

## Test Strategy

### Unit Tests (in-file, `#[cfg(test)]`)

```rust
// inst.rs tests
#[test]
fn test_encode_add() {
    // add x1, x2, x3 = 0b0000000_00011_00010_000_00001_0110011
    assert_eq!(encode_add(1, 2, 3), 0x003100b3);
}

#[test]
fn test_encode_auipc_jalr_pair() {
    // auipc x1, 0 = 0x00000097
    // jalr x1, x1, 0 = 0x000080e7
    let auipc = encode_auipc(1, 0);
    let jalr = encode_jalr(1, 1, 0);
    assert_eq!(auipc, 0x00000097);
    assert_eq!(jalr, 0x000080e7);
}
```

### Integration Test (tests/emit_tests.rs)

```rust
#[test]
fn test_simple_add_emits_valid_elf() -> Result<(), Box<dyn std::error::Error>> {
    // Build simple IrFunction: iadd -> return
    let ir = build_test_function("add x10, x10, x11");
    
    let engine = NativeEngine::new();
    let module = engine.compile(&ir, &NativeCompileOptions::default())?;
    
    // Write to /tmp for objdump inspection
    let path = "/tmp/test_simple_add.o";
    std::fs::write(path, &module.elf)?;
    
    // Verify ELF header
    let obj = object::File::parse(&*module.elf)?;
    assert_eq!(obj.architecture(), object::Architecture::Riscv32);
    
    // Optional: shell out to riscv64-unknown-elf-objdump for visual validation
    // (skipped if tool not available)
    
    Ok(())
}
```

## References

See `docs/roadmaps/2026-04-07-lpvm-native-poc/references.md` for detailed prior art:
- Cranelift fork encoding functions
- Cranelift object backend (ELF generation)
- QBE emit.c (frame layout patterns)
- RISC-V psABI (relocation types)
