# M4: LPVM Emu - Design

## Scope

Implement LPVM trait support for RV32 emulator:
1. Add shared memory region to `lp-riscv-emu` Memory (0x40000000)
2. Create new `lp-shader/lpvm-emu` crate with `EmuEngine`, `EmuModule`, `EmuInstance`
3. Remove `riscv32-emu` feature and `emu_run.rs` from `lpvm-cranelift`
4. Update `lps-filetests` to depend on `lpvm-emu` temporarily

## File Structure

```
lp-riscv/lp-riscv-emu/src/emu/
├── memory.rs                 # UPDATE: Add shared memory region at 0x40000000
└── mod.rs                    # (re-exports)

lp-shader/lpvm-emu/         # NEW: EmuEngine/EmuModule/EmuInstance + LPVM traits
├── Cargo.toml              # NEW: Dependencies (lp-riscv-emu, lpvm-cranelift, lpvm, lpir, lps-shared)
└── src/
    ├── lib.rs              # NEW: Public exports
    ├── engine.rs           # NEW: EmuEngine implements LpvmEngine
    ├── module.rs           # NEW: EmuModule implements LpvmModule
    ├── instance.rs         # NEW: EmuInstance implements LpvmInstance
    ├── memory.rs           # NEW: EmuMemory implements LpvmMemory (bump allocator)
    └── compile.rs          # NEW: RV32 compilation helpers (wraps lpvm-cranelift)

lp-shader/lpvm-cranelift/
├── Cargo.toml              # UPDATE: Remove lp-riscv-emu, lp-riscv-elf deps; remove riscv32-emu feature
└── src/
    └── emu_run.rs          # REMOVE: Moved to lpvm-emu

lp-shader/lps-filetests/
└── Cargo.toml              # UPDATE: Add lpvm-emu dependency (temporary)
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           LPVM Trait Family                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌──────────────┐    ┌──────────────┐    ┌──────────────┐                 │
│   │  LpvmEngine  │    │  LpvmModule  │    │ LpvmInstance │                │
│   │  (compile)   │    │ (instantiate)│    │    (call)    │                │
│   └──────┬───────┘    └──────┬───────┘    └──────┬───────┘                │
│          │                   │                   │                        │
│          ▼                   ▼                   ▼                        │
│   ┌─────────────────────────────────────────────────────────┐             │
│   │              lpvm-emu (NEW)                            │             │
│   │  ┌──────────┐   ┌──────────┐   ┌──────────────┐         │             │
│   │  │EmuEngine │──▶│EmuModule │──▶│  EmuInstance │         │             │
│   │  │(bump    │   │(ElfLoad  │   │(Riscv32Emu   │         │             │
│   │  │ shared  │   │  Info)   │   │  + VmCtx)    │         │             │
│   │  │ memory) │   │          │   │              │         │             │
│   │  └────┬─────┘   └──────────┘   └──────┬───────┘         │             │
│   │       │                              │                  │             │
│   │       ▼                              ▼                  │             │
│   │  ┌──────────────────────────────────────────┐           │             │
│   │  │         lp-riscv-emu Memory (NEW)        │           │             │
│   │  │   Code: 0x0        ───────────┐          │           │             │
│   │  │   Shared: 0x40000000 ────────┼──────┐   │           │             │
│   │  │   RAM: 0x80000000   ─────────┘      │   │           │             │
│   │  │                                       │   │           │             │
│   │  │   VmContext allocated in shared region│   │           │             │
│   │  └──────────────────────────────────────────┘           │             │
│   └─────────────────────────────────────────────────────────┘             │
│                           │                                                 │
│                           │ delegates RV32 codegen                          │
│                           ▼                                                 │
│                  ┌─────────────────┐                                        │
│                  │ lpvm-cranelift │  (object_bytes_from_ir)                │
│                  │   (cleaner)    │                                        │
│                  └─────────────────┘                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Key Decisions

1. **Shared memory address**: 0x40000000 (1GB mark, between code 0x0 and RAM 0x80000000)
2. **riscv32-emu removal**: Remove from `lpvm-cranelift`, `lps-filetests` temporarily depends on `lpvm-emu` until M5
3. **Allocation strategy**: Fixed bump allocator in shared region (256KB default)
4. **VmContext**: Allocated in shared memory region (fixes bug where host stack was used)
5. **Module state**: Store `ElfLoadInfo` (ready-to-run after compile)
6. **Testing**: Both unit tests (lp-riscv-emu) and integration tests (lpvm-emu)

## Dependencies

- M1 (trait redesign) - LPVM trait signatures defined
- M3 (Cranelift update) - RV32 object codegen API stable
- Existing `lp-riscv-emu` crate with two-region Memory
