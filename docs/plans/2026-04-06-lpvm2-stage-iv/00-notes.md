# M4: LPVM Emu - Notes

## Scope

Per [M4 roadmap](../../roadmaps/2026-04-06-lpvm2/m4-lpvm-emu.md):

1. **lp-riscv-emu changes**: Add third memory region (shared memory) at address 0x40000000
   - Update `Memory` struct with new shared memory region
   - Update all memory access methods for three-way dispatch (code/RAM/shared)
   - Shared memory is read-write, provided externally (reference or Arc)
   - Keep existing constructors backward-compatible

2. **New lpvm-emu crate**: `lp-shader/lpvm-emu/`
   - `EmuEngine`: owns shared memory Vec, provides bump allocator
   - `EmuModule`: holds compiled RV32 object code, symbol map, traps
   - `EmuInstance`: creates `Riscv32Emulator` with code, RAM, shared memory ref
   - Implements `LpvmEngine`, `LpvmModule`, `LpvmInstance` traits
   - Moves `emu_run.rs` logic from `lpvm-cranelift`

3. **lpvm-cranelift cleanup**:
   - Remove `emu_run.rs` and `lp-riscv-emu` dependency
   - Remove `riscv32-emu` feature
   - `LpirRv32Executable` continues working via old path until M5

## Current State

### lp-riscv-emu Memory

`lp-riscv/lp-riscv-emu/src/emu/memory.rs`:
- `Memory` struct has `code: Vec<u8>` and `ram: Vec<u8>`
- Code starts at 0x0, RAM at 0x80000000 (DEFAULT_RAM_START)
- All access methods (read_word, write_word, etc.) dispatch to code or RAM
- No shared memory region currently

### lpvm-cranelift emu dependencies

`lp-shader/lpvm-cranelift/Cargo.toml`:
- `riscv32-emu` feature enables `lp-riscv-emu` and `lp-riscv-elf` deps
- `emu_run.rs` (behind `riscv32-emu` feature) contains:
  - `glsl_q32_call_emulated()` - Q32 typed calls through linked RV32
  - `run_lpir_function_i32()` - compile → link → emulate helpers
  - Tests requiring `lps-builtins-emu-app`

### lps-filetests dependency

`lp-shader/lps-filetests/src/test_run/lpir_rv32_executable.rs`:
- `LpirRv32Executable` uses `lpvm_cranelift::glsl_q32_call_emulated`
- Uses `object_bytes_from_ir`, `link_object_with_builtins`
- Implements `GlslExecutable` trait (old trait, not LPVM traits)

### LPVM traits available

`lp-shader/lpvm/src/`:
- `engine.rs`: `LpvmEngine` trait with `compile()` and `memory()`
- `module.rs`: `LpvmModule` trait with `instantiate()`
- `instance.rs`: `LpvmInstance` trait with `call()`
- `memory.rs`: `LpvmMemory` trait with `alloc/free/realloc`

## Questions

### Q1: Shared memory address range confirmation ✅

**Answer**: Use 0x40000000 as proposed.

### Q2: riscv32-emu feature removal strategy ✅

**Answer**: Option B - keep `riscv32-emu` temporarily with re-exports from `lpvm-emu`. Mark deprecated. Ensure cleanup milestone (M7) explicitly calls this out for removal.

**Note**: The roadmap `m7-cleanup.md` already lists "Remove any remaining `emu_run.rs` remnants from `lpvm-cranelift`" - this covers it.

### Q3: Shared memory allocation strategy in EmuEngine ✅

**Answer**: Option A - fixed bump allocator. Configurable size, default 256KB (matching existing `BumpLpvmMemory` pattern).

### Q4: How to handle VmContext in EmuInstance ✅

**Answer**: Option B - allocate VmContext in shared memory.

**Insight**: Current host stack allocation is a bug - guest cannot access host stack memory. VmContext must be in guest-addressable memory (the shared region) for the guest to actually use it.

Implementation: `EmuInstance` will allocate `VmContextHeader` in the shared memory region during instantiation.

### Q5: lpvm-emu dependency graph ✅

**Answer**: Option B - `lps-filetests` depends directly on `lpvm-emu` during transition. No circularity. `lpvm-cranelift` removes `riscv32-emu` entirely.

**Updated dependency flow**:
```
lpvm-emu → lpvm-cranelift (object codegen)
lpvm-emu → lp-riscv-emu (emulator)
lps-filetests → lpvm-emu (temporary, until M5)
```

### Q6: Testing strategy for shared memory ✅

**Answer**: Option C - both. Unit tests in `lp-riscv-emu` for three-way dispatch; integration tests in `lpvm-emu` for trait compliance and shader execution.

### Q7: EmuModule compilation delegation ✅

**Answer**: Option A - store `ElfLoadInfo`. Compilation and linking happen in `EmuEngine::compile()`, producing a ready-to-run module. Each `EmuInstance` gets its own RAM copy but shares code and symbol map.

---

## Decisions Summary

| Question | Decision | Notes |
|----------|----------|-------|
| Q1 | 0x40000000 | Shared memory address range |
| Q2 | Temp re-exports via lps-filetests | lpvm-cranelift removes riscv32-emu entirely |
| Q3 | Fixed bump allocator | 256KB default, matches BumpLpvmMemory |
| Q4 | VmContext in shared memory | Current host stack is a bug |
| Q5 | lps-filetests → lpvm-emu directly | No circular dependencies |
| Q6 | Both unit + integration tests | Thorough coverage |
| Q7 | Store ElfLoadInfo | Ready-to-run module pattern |
