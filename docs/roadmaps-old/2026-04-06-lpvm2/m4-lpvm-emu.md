# Milestone 4: lpvm-emu

## Goal

Add shared memory region support to `lp-riscv-emu` and create the `lpvm-emu`
crate with `EmuEngine`/`EmuModule`/`EmuInstance` implementing LPVM traits.
Move `emu_run.rs` from `lpvm-cranelift` to `lpvm-emu`.

## Suggested plan name

`lpvm2-m4`

## Scope

### In scope

#### lp-riscv-emu changes

- Add a third memory region to `Memory` struct: shared memory at a known
address range (e.g., 0x40000000)
- Update all memory access methods (`read_word`, `write_word`, `read_byte`,
`write_byte`, `read_halfword`, `write_halfword`, `read_u8`,
`fetch_instruction`) with the three-way address dispatch
- Shared memory is read-write, provided externally (reference or Arc)
- Existing constructors remain backward-compatible (shared memory is optional)
- Tests for the new memory region

#### New lpvm-emu crate

- Create `lp-shader/lpvm-emu/` crate
- `EmuEngine`: owns the shared memory `Vec<u8>`, provides `alloc()`/`free()`
as a bump allocator within the shared region
- `EmuModule`: holds compiled RV32 object code (code bytes, symbol map,
traps). Compilation delegates to `lpvm-cranelift` for RV32 codegen.
- `EmuInstance`: creates a `Riscv32Emulator` with the module's code, its own
RAM, and a reference to the engine's shared memory
- Implement `LpvmEngine`, `LpvmModule`, `LpvmInstance` traits
- Move `emu_run.rs` logic from `lpvm-cranelift` to `lpvm-emu`
- Unit tests: compile shader → create instance → call function → get result

#### lpvm-cranelift cleanup

- Remove `emu_run.rs` and the `lp-riscv-emu` dependency from `lpvm-cranelift`
- Remove the `riscv32-emu` feature flag if it exists
- Existing consumers of `emu_run.rs` (filetests via `LpirRv32Executable`)
continue to work via the old path until M5 migrates them

### Out of scope

- Emulator fork/rewrite (no longer needed)
- Full firmware ELF loading via LPVM (fw-tests stay on direct emulator API)
- Multiple concurrent instances sharing memory (structural support is there,
but testing focuses on single-instance correctness)

## Key Decisions

1. **Shared memory address range**: 0x40000000 is between code (0x0) and RAM
  (0x80000000). Must verify the RV32 linker script doesn't place anything
   in this range.
2. **Shared memory is optional in lp-riscv-emu**: The existing emulator API
  continues to work without shared memory. Only `lpvm-emu` provides shared
   memory when creating emulator instances.
3. **emu_run.rs moves, not copies**: The object compilation + linking +
  emulator setup code moves from `lpvm-cranelift/src/emu_run.rs` to
   `lpvm-emu`. `lpvm-emu` depends on `lpvm-cranelift` for RV32 object
   codegen (the `compile_to_rv32_object` function or equivalent).
4. **Backward compatibility**: `LpirRv32Executable` in `lps-filetests`
  continues to work via the old code path until M5 migrates filetests
   to LPVM traits. The old `emu_run.rs` path may be temporarily duplicated
   or re-exported during migration.

## Deliverables

- Updated `lp-riscv/lp-riscv-emu/src/emu/memory.rs` — shared memory region
- New `lp-shader/lpvm-emu/` crate with Cargo.toml, src/lib.rs, engine, module,
instance modules
- Updated `lp-shader/lpvm-cranelift/Cargo.toml` — remove lp-riscv-emu dep
- Tests for emulator shared memory and LPVM trait compliance

## Dependencies

- Milestone 1 (trait redesign) — trait signatures
- Milestone 3 (Cranelift update) — needed for RV32 object codegen API to
be stable (lpvm-emu calls into lpvm-cranelift for compilation)

## Estimated scope

~~500–800 lines. The emulator change is small (~~50 lines for three-way
dispatch). The `lpvm-emu` crate is the bulk, largely restructuring existing
`emu_run.rs` code behind LPVM traits.