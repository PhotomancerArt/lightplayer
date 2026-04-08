# M2.1 Debugging Infrastructure Plan Notes

## Scope of Work

M2.1 is an interstitial milestone focused on debugging infrastructure for `lpvm-native`. Before proceeding to M3 (execution/integration), we need robust tooling to inspect and debug generated code.

### In Scope
- LPIR-to-assembly correlation tracking (op indices → instruction offsets)
- Human-readable annotated assembly output (labels, comments showing LPIR origin)
- Optional hex offsets for emulator/core-dump correlation
- Compiler flag to enable/disable debug tracking (disable on embedded for speed)
- `lp-cli` command to compile shader to annotated RV32 assembly
- Unit tests that verify annotated assembly output
- Infrastructure to support future DWARF and GLSL debugger

### Out of Scope
- Full DWARF emission (design for it, but don't implement)
- Interactive debugger (just output format that could support it)
- Integration with filetest harness (M3 concern)

## Current State

### Existing Tools in `lp-riscv-inst`
- `decode_instruction()` - decode RV32 bytes to `Inst` enum
- `format_instruction()` - format `Inst` as assembly string (e.g., "add a0, a1, a2")
- `Inst::format()` method on all instruction variants
- Full R/I/S/B/U/J type decoding

### Current `lpvm-native` Emitter
- `EmitContext::emit_vinst()` maps VInst to RV32 instructions
- `EmitContext::code: Vec<u8>` holds raw bytes
- `EmitContext::relocs: Vec<NativeReloc>` holds relocations
- No tracking of which LPIR op generated which instruction
- No human-readable output beyond raw ELF

### Missing
- Correlation between LPIR ops and instruction offsets
- Assembly dump with annotations
- Labels for function entry/relocations
- CLI tool for assembly inspection

## Questions

### Q1: Debug tracking - when to enable?

**Context:** We need debug tracking for development but want to disable on embedded for speed/size.

**Options:**
- A: `#[cfg(feature = "debug-info")]` - compile-time flag
- B: `NativeCompileOptions { debug_info: bool }` - runtime flag, stored in context
- C: Always track, strip in release builds via dead code elimination
- D: Separate `DebugEmitContext` type, only used by debug CLI

**Answer:** B - Runtime flag in `NativeCompileOptions`. Allows:
- Host/CLI builds with debug info enabled
- Embedded builds with `debug_info: false` (no overhead)
- Same code paths, just skip recording when disabled

### Q2: Tracking data structure

**Context:** Need to record which LPIR op (and ideally which function/block) generated which instruction(s).

**Options:**
- A: `Vec<(u32, Option<u32>)>` in EmitContext - (offset, src_op_index)
- B: Store `src_op: u32` in each VInst, record ranges
- C: Full line table: `struct LineEntry { offset: u32, src_op: u32, func: u32, block: u32 }`
- D: Sparse map: `BTreeMap<u32, SourceLoc>` - offset → location lookup

**Answer:** B + A combination:
- Add `src_op: Option<u32>` to VInst (propagated during lowering)
- Record `(offset, src_op)` in EmitContext during emission
- Keep it simple but extensible for future DWARF

### Q3: Assembly output format

**Context:** Need both human-readable and machine-parseable output. Want labels, LPIR annotations, and optionally hex offsets.

**Desired output example:**
```asm
        .globl  add
        .type   add, @function
add:                                    # func @add(v1:f32, v2:f32) -> f32
        addi    sp, sp, -16             # prologue
        sw      ra, 12(sp)
        # LPIR[2]: v3 = fadd v1, v2
        lui     a3, %hi(__lp_lpir_fadd_q32)
        addi    a3, a3, %lo(__lp_lpir_fadd_q32)
        mv      a0, a1                  # v1 -> arg0
        mv      a1, a2                  # v2 -> arg1
        jalr    ra, a3                  # call __lp_lpir_fadd_q32
        # LPIR[3]: return v3
        lw      ra, 12(sp)              # epilogue
        addi    sp, sp, 16
        ret
        .size   add, .-add
```

**Options:**
- A: Custom text format (as above)
- B: Standard GNU assembler format (`.s` file compatible)
- C: JSON with structure for tooling
- D: Both A and B (internal pretty print + gas output)

**Answer:** A with B-compatible syntax - Use standard asm mnemonics and directives, but our own annotation style. This is:
- Human readable
- Close enough to feed through `riscv64-unknown-elf-as` if needed
- Simple to generate

### Q4: Where to place disassembly logic?

**Context:** We have `lp-riscv-inst` for decoding, `lpvm-native` for emission. Need assembly output somewhere.

**Options:**
- A: New module in `lpvm-native` (`debug/disasm.rs`)
- B: Extend `lp-riscv-inst` with annotation-aware formatting
- C: New crate `lp-riscv-asm` for assembly I/O
- D: Part of `EmitContext` as `disassemble()` method

**Answer:** A - Keep it in `lpvm-native/src/debug/`:
- `debug/mod.rs` - DebugInfo struct, tracking
- `debug/disasm.rs` - Disassembly with annotations
- `debug/write.rs` - Output formatting (text, maybe future DWARF)
- Tightly coupled to our emission, can evolve together

### Q5: CLI interface

**Context:** Need `lp-cli` command to compile shader → annotated assembly.

**Options:**
- A: `lp-cli shader-asm <file.glsl> [--output out.s]`
- B: Extend existing `lp-cli shader-lpir` with `--emit=asm` option
- C: `lp-cli compile --target=rv32-asm <file.glsl>`

**Answer:** A - `lp-cli shader-rv32 <file.glsl>`:
- Simple, clear purpose
- Can add `--format=gas/json` later
- Follows pattern of `shader-lpir` subcommand

### Q6: Core-dump correlation (future-proofing)

**Context:** You mentioned emulator core dumps showing better renderings. Need to support PC → assembly mapping.

**Options:**
- A: Just hex offsets in comments (e.g., `# 0x0010: add sp, sp, -16`)
- B: Generate separate symbol file (function → offset ranges)
- C: Design line table structure now, implement minimal
- D: Full DWARF `.debug_line` (too heavy for M2.1)

**Answer:** C - Design `LineTable` structure now:
```rust
// In debug/mod.rs
pub struct LineTable {
    /// Sorted by offset, binary searchable
    entries: Vec<LineEntry>,
}

pub struct LineEntry {
    offset: u32,      // Instruction offset in code section
    src_op: u32,      // LPIR operation index
    // Future: src_file, src_line for DWARF
}

impl LineTable {
    /// Find the source op for a given PC offset
    pub fn lookup(&self, offset: u32) -> Option<&LineEntry> {
        // Binary search for offset
    }
}
```
- Use it for assembly output
- Can be serialized for emulator use
- Foundation for DWARF `.debug_line` later

## Notes

### Prior Art Analysis

**Cranelift approach:**
- Debug tags attached to CLIF instructions, lowered to VCode
- Tags are opaque to Cranelift except during lowering translation
- `MachDebugTagPos`, `MachBufferDebugTagList` track positions in the buffer
- DWARF section writing with `WriterRelocate` structure
- Handles relocations, address/offset writing, endianness
- Preserves debug info through optimizations and inlining
- StackSlot variants translated to physical stackframe offsets

**QBE approach:**
- Simpler: `dbgfile` and `dbgloc` directives in the IL
- `dbgloc` includes line and column information (enhanced 2024)
- Line number tracking implemented in per-backend `emit.c`
- Maps QBE IL instructions to source locations at emission time

**For `lpvm-native`:**
Take a hybrid approach:
- Like QBE: Simple source location in VInst (`src_op` index from LPIR)
- Like Cranelift: Track positions during emission (offset → src_op mapping)
- Future: Extend line table to full DWARF `.debug_line` if needed

### Future DWARF Considerations

The tracking infrastructure should be designed to eventually support:
- `.debug_line` - mapping PC → source file/line (we'll have PC → LPIR op, need GLSL source mapping)
- `.debug_info` - function info, types
- `.debug_abbrev` - abbreviation tables

For now, we track minimal info that can be extended:
```rust
pub struct SourceLoc {
    pub lpir_op: u32,
    // Future: glsl_line: u32, glsl_col: u32, etc.
}
```

### Assembly Label Handling

For re-assemblable output, we need to handle:
- Relocations as labels (e.g., `__lp_lpir_fadd_q32`)
- Function entry labels (function names)
- Local labels for branches (`.L1`, `.L2`, etc. - not needed for M2.1 simple tests)

The `NativeReloc` already has symbol names - just need to emit as `.globl` + label.

### Testing Strategy

- Unit test: Compile simple LPIR, verify assembly contains expected annotations
- Integration test: `lp-cli shader-asm` produces readable output
- Golden file tests: Compare assembly output to expected (stable format)

### Validation

```bash
# Compile to assembly
cargo run -p lp-cli -- shader-asm filetests/scalar/int/op-add.glsl

# Expected: Human-readable annotated assembly
# Check: Contains function name, prologue comment, LPIR annotations

# No-std check (debug disabled path)
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf
```
