# Extract lp-riscv-emu-guest Crate - Design

## Scope of Work

Extract the RISC-V32 emulator guest code from `lp-glsl/apps/lp-builtins-app` into a new common crate
`lp-glsl/crates/lp-riscv-emu-guest`. This guest code provides the runtime foundation for code
running in the RISC-V emulator and needs to be reusable by firmware and other applications.

## File Structure

```
lp-glsl/crates/lp-riscv-emu-guest/          # NEW: Common guest runtime crate
├── Cargo.toml
├── build.rs                           # Sets up memory.ld linker script
├── memory.ld                          # Linker script for memory layout
└── src/
    ├── lib.rs                         # Public API exports
    ├── entry.rs                       # Entry point (_entry, _code_entry)
    ├── panic.rs                       # Panic handler with syscall reporting
    ├── syscall.rs                     # Syscall implementation (internal)
    ├── host.rs                        # Host communication (__host_debug, __host_println)
    └── print.rs                       # Print macros and writer

lp-glsl/apps/lp-builtins-app/         # UPDATE: Thin binary wrapper
├── Cargo.toml                         # UPDATE: Add dependency on lp-riscv-emu-guest
├── build.rs                           # UPDATE: Remove (linker script now in crate)
└── src/
    ├── main.rs                        # UPDATE: Thin wrapper, calls lp-riscv-emu-guest entry
    └── builtin_refs.rs                # KEEP: App-specific builtin references
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    lp-riscv-emu-guest                        │
│  (Common RISC-V32 emulator guest runtime)              │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Entry Point Flow:                                      │
│    _entry (assembly)                                    │
│      ↓                                                  │
│    _code_entry (bootstrap: init .bss/.data)           │
│      ↓                                                  │
│    Calls app-specific main (e.g., _lp_main)            │
│                                                         │
│  Runtime Services:                                      │
│    • Panic handler (syscall-based)                     │
│    • Syscall implementation (ecall)                    │
│    • Host communication (debug, println)               │
│    • Print macros (no_std compatible)                  │
│                                                         │
└─────────────────────────────────────────────────────────┘
                        ↑
                        │ depends on
                        │
┌─────────────────────────────────────────────────────────┐
│                 lp-builtins-app                        │
│  (Thin binary wrapper for builtins library)            │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  App-Specific:                                         │
│    • _lp_main() - references all builtins              │
│    • builtin_refs.rs - auto-generated references        │
│                                                         │
│  Uses:                                                  │
│    • lp-riscv-emu-guest entry point                          │
│    • lp-riscv-emu-guest host/print modules                   │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

## Main Components and Interactions

### lp-riscv-emu-guest Crate

**Purpose**: Provides reusable runtime foundation for RISC-V32 emulator guest code.

**Key Components**:

1. **Entry Point (`entry.rs`)**:
    - `_entry`: Assembly entry point that initializes GP, SP, FP
    - `_code_entry`: Bootstrap function that initializes .bss and .data sections, then calls app
      main

2. **Panic Handler (`panic.rs`)**:
    - Formats panic messages
    - Reports panics to host via syscall
    - Calls `ebreak` to halt execution

3. **Syscall Implementation (`syscall.rs`)**:
    - Internal module (not public API)
    - Implements `ecall` instruction wrapper
    - Handles syscall argument passing

4. **Host Communication (`host.rs`)**:
    - Public module
    - `__host_debug`: Debug output (respects DEBUG env var)
    - `__host_println`: Always-print output
    - Used by macros for host communication

5. **Print Macros (`print.rs`)**:
    - Public module
    - `print!`, `println!` macros for no_std environments
    - Writer implementation that uses syscalls

6. **Public API (`lib.rs`)**:
    - Re-exports `host` and `print` modules
    - Re-exports macros: `print!`, `println!`, `host_debug!`, `host_println!`
    - Entry point functions are `#[no_mangle]` so they're accessible from binaries

### lp-builtins-app Refactoring

**Purpose**: Thin binary wrapper that links all builtins into a static library.

**Changes**:

- Depends on `lp-riscv-emu-guest` crate
- Removes most code (moved to crate)
- Keeps `_lp_main()` function that references builtins
- Keeps `builtin_refs.rs` (app-specific)
- Calls entry point from `lp-riscv-emu-guest`

**Entry Flow**:

1. `_entry` (from `lp-riscv-emu-guest`) initializes registers
2. `_code_entry` (from `lp-riscv-emu-guest`) initializes memory sections
3. `_code_entry` calls `_lp_main()` (from `lp-builtins-app`)
4. `_lp_main()` references builtins and jumps to user code

## Design Decisions

1. **Library Crate**: `lp-riscv-emu-guest` is a library crate, not a binary. Applications link
   against it.

2. **Linker Script**: `memory.ld` is included in the crate. Applications can override if needed.

3. **Builtin References**: Remain app-specific. Not included in `lp-riscv-emu-guest`.

4. **Public API**: Controlled API surface with public modules (`host`, `print`) and entry functions.

5. **No Feature Flags**: Start simple, add features later if needed.

6. **Thin Wrapper**: `lp-builtins-app` becomes a minimal binary that depends on the crate.
