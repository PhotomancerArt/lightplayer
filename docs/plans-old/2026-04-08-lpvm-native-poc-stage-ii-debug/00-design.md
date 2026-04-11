# M2.1 Debug Infrastructure Design

## Scope

M2.1 adds debugging infrastructure to `lpvm-native` for tracking and displaying LPIR-to-RV32 code correlation. This enables:
- Annotated assembly output showing which LPIR operation generated each instruction
- Human-readable disassembly with labels and comments
- PC → source mapping for emulator debugging and core dumps
- Foundation for future DWARF debug info

### In Scope
- Add `src_op` tracking to VInst (LPIR operation index)
- Track (offset, src_op) pairs during emission
- LineTable structure for binary-searchable PC lookup
- Annotated disassembly (RV32 bytes → readable assembly with LPIR comments)
- `lp-cli shader-rv32 <file.glsl>` command

### Out of Scope
- Full DWARF emission (design for it, but don't implement)
- Interactive debugger
- Integration with filetest harness (M3 concern)
- Optimized debug info (minimal viable first)

## File Structure

```
lp-shader/lpvm-native/
└── src/
    ├── lib.rs                          # UPDATE: Add `pub mod debug` feature-gated
    ├── vinst.rs                        # UPDATE: Add `src_op: Option<u32>` to variants
    ├── lower.rs                        # UPDATE: Pass op indices during lowering
    ├── engine.rs                       # UPDATE: Add `debug_info: bool` to options
    ├── isa/
    │   └── rv32/
    │       ├── mod.rs                  # UPDATE: Add `pub mod debug` re-export
    │       ├── emit.rs                 # UPDATE: Track (offset, src_op) when emitting
    │       └── debug/                  # NEW: Debug infrastructure
    │           ├── mod.rs              # NEW: DebugInfo, LineTable, LineEntry
    │           ├── disasm.rs           # NEW: Annotated disassembly
    │           └── write.rs            # NEW: Text assembly output format
    └── tests/
        └── debug_asm.rs                # NEW: Test annotated assembly output

lp-cli/
└── src/
    ├── commands/
    │   ├── mod.rs                      # UPDATE: Add `shader_rv32` module
    │   └── shader_rv32/                # NEW: GLSL → annotated RV32 assembly
    │       ├── mod.rs
    │       ├── args.rs
    │       └── handler.rs
    └── main.rs                         # UPDATE: Add `shader-rv32` subcommand
```

## Conceptual Architecture

### Data Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                        Debug Pipeline                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  LPIR Module                                                    │
│     │                                                           │
│     ▼                                                           │
│  lower() ──────────┐                                            │
│     │              │                                            │
│     │ src_op index │                                            │
│     ▼              │                                            │
│  VInst::Add32 {    │                                            │
│     dst, src1,     │                                            │
│     src2,          │                                            │
│     src_op: Some(5)│ ◄── track source LPIR op index              │
│  }                 │                                            │
│                    │                                            │
│  allocate() ────────┤ (preserve src_op through regalloc)          │
│     │              │                                            │
│     ▼              │                                            │
│  emit_vinst()      │                                            │
│     │              │                                            │
│     │ record       │                                            │
│     │ (offset, 5) ─┘                                            │
│     ▼                                                           │
│  EmitContext {                                                  │
│     code: Vec<u8>,                                              │
│     debug_lines: Vec<(u32, u32)>,  ◄── offset → src_op           │
│  }                                                              │
│     │                                                           │
│     ▼                                                           │
│  LineTable {                                                    │
│     entries: [                                                  │
│        LineEntry { offset: 0, src_op: 0 },  # prologue          │
│        LineEntry { offset: 4, src_op: 2 },  # LPIR[2]           │
│        ...                                                      │
│     ]                                                           │
│  }                                                              │
│     │                                                           │
│     ▼                                                           │
│  disassemble()                                                  │
│     │                                                           │
│     ▼                                                           │
│  Annotated Assembly                                             │
│  ═══════════════════                                            │
│  .globl  add                                                    │
│  add:                              # func @add(...)             │
│      addi sp, sp, -16              # prologue                   │
│      # LPIR[2]: v3 = fadd v1, v2                                │
│      lui  a3, %hi(__lp_lpir_fadd_q32)                           │
│      ...                                                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Key Components

#### 1. Source Tracking in VInst

Each VInst variant carries an optional `src_op` field identifying the originating LPIR operation:

```rust
pub enum VInst {
    Add32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,  // LPIR operation index
    },
    // ... other variants
}
```

This is set during lowering by passing the op index from the LPIR `body: Vec<Op>` iteration.

#### 2. Debug Line Tracking in EmitContext

When `debug_info` is enabled in `NativeCompileOptions`, the `EmitContext` records:

```rust
pub struct EmitContext {
    code: Vec<u8>,
    relocs: Vec<NativeReloc>,
    frame_size: i32,
    is_leaf: bool,
    // Debug tracking (only populated when debug_info enabled)
    debug_lines: Vec<(u32, Option<u32>)>,  // (offset, src_op)
}
```

Each `push_u32()` call records the current offset and the current VInst's `src_op`.

#### 3. LineTable for PC Lookup

The debug lines are converted to a sorted, binary-searchable table:

```rust
pub struct LineTable {
    entries: Vec<LineEntry>,
}

pub struct LineEntry {
    offset: u32,      // Instruction offset in code section
    src_op: u32,      // LPIR operation index
}

impl LineTable {
    /// Binary search for the LineEntry at or before the given offset
    pub fn lookup(&self, pc: u32) -> Option<&LineEntry> {
        // Binary search for largest offset <= pc
    }
}
```

This enables:
- Assembly annotation: "# LPIR[5]: v3 = add v1, v2"
- Emulator debugging: PC 0x44 → LineEntry { offset: 0x44, src_op: 5 } → LPIR op 5

#### 4. Disassembly with Annotations

The `disasm.rs` module uses `lp_riscv_inst::decode_instruction()` to decode RV32 bytes and produces annotated output:

```rust
pub fn disassemble_with_annotations(
    code: &[u8],
    line_table: &LineTable,
    lpir_module: &IrModule,  // For formatting LPIR ops
    function: &IrFunction,
) -> String {
    // For each 4-byte instruction:
    // 1. Decode to Inst
    // 2. Look up src_op in LineTable
    // 3. Format: "    <mnemonic>    # LPIR[<n>]: <lpir_op>"
}
```

#### 5. CLI Integration

The `lp-cli shader-rv32` command:
1. Parses GLSL via `lps_frontend`
2. Lowers to LPIR
3. Compiles via `lpvm_native` with `debug_info: true`
4. Generates annotated assembly via `disassemble_with_annotations()`
5. Writes to stdout or file

## Prior Art

### Cranelift Approach
- Debug tags attached to CLIF instructions, lowered to VCode
- `MachDebugTagPos`, `MachBufferDebugTagList` track positions
- DWARF section writing via `WriterRelocate`
- Preserves debug info through optimizations

### QBE Approach
- `dbgfile` and `dbgloc` directives in IL
- Line/column tracking in backend `emit.c`
- Simple, direct mapping

### Our Hybrid
- Like QBE: Simple `src_op` in VInst (per-instruction tracking)
- Like Cranelift: Position tracking during emission (offset → src_op)
- Future: LineTable can extend to full DWARF `.debug_line`

## Future DWARF Considerations

The `LineTable` structure is designed for extension:

```rust
pub struct LineEntry {
    offset: u32,
    src_op: u32,
    // Future additions:
    // file_id: u32,      // For .debug_line file table
    // line: u32,         // GLSL source line
    // column: u32,       // GLSL source column
}
```

Future work:
- Generate `.debug_line` section during ELF emission
- Support GLSL source file/line/column tracking
- Integrate with GDB/lldb for shader debugging

## Success Criteria

```bash
# Unit test: Native module compiles and produces debug info
cargo test -p lpvm-native --lib debug_asm

# CLI test: Compile shader to annotated assembly
cargo run -p lp-cli -- shader-rv32 filetests/scalar/int/op-add.glsl

# Expected output contains:
# .globl  add
# add:
#     addi sp, sp, -16
#     # LPIR[2]: v3 = fadd v1, v2
#     ...

# No-std check: debug disabled path compiles for embedded
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf
```
