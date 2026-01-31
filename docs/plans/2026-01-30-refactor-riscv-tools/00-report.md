# Study: Refactor lp-riscv-tools Structure

## Executive Summary

`lp-riscv-tools` is currently a mixed-purpose crate that combines:

1. **Core emulator** (mostly `no_std` compatible)
2. **ELF loading/linking** (requires `std`)
3. **Instruction utilities** (decode/encode, `no_std` compatible)
4. **Serial communication** (`no_std` compatible)

The crate uses a `std` feature flag to gate std-dependent functionality, but the structure is
confusing because:

- The crate is `#![no_std]` by default
- Some core emulator functionality (time tracking, debug output) conditionally uses `std`
- ELF loading is completely gated behind `std` feature
- Consumers have different needs: `fw-emu` needs `no_std`, `lp-glsl-compiler` needs `std`

## Current Structure

### Package Overview

```
lp-riscv/lp-riscv-tools/
├── Cargo.toml          # no_std by default, std feature gates ELF loading
├── src/
│   ├── lib.rs          # Main entry point, conditionally exports ELF loader
│   ├── emu/            # Emulator core
│   │   ├── mod.rs
│   │   ├── executor.rs         # Instruction execution (mostly no_std)
│   │   ├── memory.rs            # Memory model (no_std)
│   │   ├── logging.rs           # Logging types (no_std)
│   │   ├── error.rs             # Error types (no_std)
│   │   ├── abi_helper.rs        # ABI utilities (no_std, uses alloc::format)
│   │   ├── decoder.rs           # Instruction decoder (no_std)
│   │   └── emulator/            # Emulator state and control
│   │       ├── mod.rs
│   │       ├── state.rs          # Core state (conditionally uses std::time::Instant)
│   │       ├── execution.rs     # Syscall handling (conditionally uses std::io::Write)
│   │       ├── run_loops.rs      # High-level run methods (no_std)
│   │       ├── function_call.rs # Function calling (no_std)
│   │       ├── registers.rs     # Register management (no_std)
│   │       ├── types.rs         # Public types (no_std)
│   │       └── debug.rs         # Debug formatting (no_std)
│   ├── elf_loader/      # ELF file loading (std only)
│   │   ├── mod.rs
│   │   ├── parse.rs
│   │   ├── layout.rs
│   │   ├── sections.rs
│   │   ├── symbols.rs
│   │   ├── memory.rs
│   │   ├── relocations/
│   │   └── object/      # Object file utilities
│   ├── elf_linker.rs   # ELF linking utilities (std only)
│   ├── serial/          # Serial communication (no_std)
│   │   ├── mod.rs
│   │   ├── host_serial.rs
│   │   └── test_serial.rs
│   ├── decode.rs        # Instruction decoding (no_std)
│   ├── decode_rvc.rs    # Compressed instruction decoding (no_std)
│   ├── encode.rs        # Instruction encoding (no_std)
│   ├── inst.rs          # Instruction types (no_std)
│   ├── format.rs        # Instruction format helpers (no_std)
│   ├── regs.rs          # Register definitions (no_std)
│   ├── register_role.rs # Register role analysis (no_std)
│   ├── auipc_imm.rs     # AUIPC immediate utilities (no_std)
│   └── debug.rs         # Debug macro (conditionally uses std)
└── tests/
    └── ...              # Various test files
```

## What Needs std and Why

### 1. ELF Loading (`elf_loader/`, `elf_linker.rs`) - **REQUIRES std**

**Why:**

- Uses `object` crate (version 0.37.3) which requires `std`
- Uses `elf` crate (version 0.7) which requires `std`
- File I/O operations for loading ELF files
- Used for loading compiled binaries into the emulator

**Current gating:**

- Entire module is `#[cfg(feature = "std")]`
- Only exported when `std` feature is enabled

**Usage:**

- `lp-glsl-compiler` uses this when `emulator` feature is enabled
- Tests use this for loading test binaries

### 2. Time Tracking (`emu/emulator/state.rs`) - **OPTIONAL std**

**Why:**

- Uses `std::time::Instant` for elapsed time calculation
- Used by `SYSCALL_TIME_MS` syscall handler
- Provides timing information for emulator runs

**Current gating:**

- Conditionally compiled: `#[cfg(feature = "std")]`
- Returns 0 when `std` feature is disabled

**Impact:**

- Core emulator functionality works without it
- Only affects timing syscall

### 3. Debug Output (`emu/emulator/execution.rs`) - **OPTIONAL std**

**Why:**

- Uses `std::io::Write` and `std::io::stderr()` for syscall output
- Used by `SYSCALL_WRITE` syscall handler
- Provides host-side output for guest programs

**Current gating:**

- Conditionally compiled: `#[cfg(feature = "std")]`
- No-op when `std` feature is disabled

**Impact:**

- Core emulator functionality works without it
- Only affects write syscall output

### 4. Debug Macro (`debug.rs`) - **OPTIONAL std**

**Why:**

- Uses `std::env::var` to check `DEBUG` environment variable
- Uses `std::eprintln!` for debug output

**Current gating:**

- Conditional macro definition based on `std` feature
- No-op when `std` feature is disabled

**Impact:**

- Only affects debug logging
- No impact on core functionality

### 5. ABI Helper (`emu/abi_helper.rs`) - **NO std REQUIRED**

**Current state:**

- Uses `alloc::format!` when `std` is not available
- Uses `std::format!` when `std` is available (for consistency)
- Fully functional in `no_std` mode

## Current Consumers

### 1. `fw-emu` (Firmware Emulator)

**Usage:**

```toml
lp-riscv-tools = { path = "../../lp-riscv/lp-riscv-tools", default-features = false }
```

**Needs:**

- Core emulator (`Riscv32Emulator`)
- Serial communication (`HostSerial`)
- **Does NOT need:** ELF loading (builds firmware directly)

**Current status:** ✅ Works with `no_std`

### 2. `lp-glsl-compiler`

**Usage:**

```toml
lp-riscv-tools = { path = "../../../lp-riscv/lp-riscv-tools", optional = true, features = ["std"] }
```

**Needs:**

- Core emulator (`Riscv32Emulator`)
- ELF loading (`load_elf`)
- Instruction utilities (`Gpr`, `Inst`, `decode_instruction`)
- **Requires:** `std` feature for ELF loading

**Current status:** ✅ Works with `std` feature enabled

### 3. Tests

**Usage:**

- Various test files use different combinations
- Some tests require `std` for ELF loading
- Some tests work with `no_std` only

## Problems with Current Structure

### 1. Mixed Responsibilities

The crate combines:

- **Core emulator** (runtime execution)
- **ELF tooling** (build-time/development tooling)
- **Instruction utilities** (could be standalone)

These have different use cases and dependencies.

### 2. Confusing Feature Gating

- Core emulator has optional `std` features (time, debug output)
- ELF loading is completely gated
- Hard to understand what works in `no_std` vs `std` mode

### 3. Naming Confusion

- Name "tools" suggests development utilities
- But core emulator is runtime component
- ELF loading is development tooling

### 4. Dependency Bloat

- Consumers that only need emulator still pull in ELF loading dependencies (even if gated)
- `object` and `elf` crates are large dependencies

## Proposed Solution: Split into Multiple Crates

### Option 1: Three-Crate Split (Recommended)

#### 1. `lp-riscv-emu` - Core Emulator

**Purpose:** Pure emulator runtime, `no_std` compatible

**Contents:**

- `emu/` module (emulator core)
- `serial/` module (serial communication)
- Optional `std` feature for:
    - Time tracking (`std::time::Instant`)
    - Debug output (`std::io::Write`)

**Dependencies:**

- `alloc` (for `Vec`, `VecDeque`, `String`)
- `cranelift-codegen` (for ABI, trap codes)
- `lp-riscv-emu-shared` (for syscall constants)
- `hashbrown` (for `HashMap` in `no_std`)

**Public API:**

- `Riscv32Emulator`
- `StepResult`, `SyscallInfo`, `PanicInfo`
- `HostSerial`
- `EmulatorError`, `MemoryAccessKind`

#### 2. `lp-riscv-inst` - Instruction Utilities

**Purpose:** Instruction decode/encode, register definitions

**Contents:**

- `decode.rs`, `decode_rvc.rs`, `encode.rs`
- `inst.rs`, `format.rs`
- `regs.rs`, `register_role.rs`
- `auipc_imm.rs`
- `debug.rs` (conditional std)

**Dependencies:**

- `alloc` (for `String` in error messages)
- No `cranelift-codegen` dependency

**Public API:**

- `Inst`, `Gpr`
- `decode_instruction()`, `encode()`
- `format_instruction()`

#### 3. `lp-riscv-elf` - ELF Tooling

**Purpose:** ELF loading and linking (development tooling)

**Contents:**

- `elf_loader/` module
- `elf_linker.rs`

**Dependencies:**

- `std` (required)
- `object` crate
- `elf` crate (optional, if still needed)
- `lp-riscv-inst` (for instruction utilities if needed)
- `hashbrown` (for `HashMap`)

**Public API:**

- `load_elf()`, `find_symbol_address()`
- `link_static_library()` (if implemented)
- `ElfLoadInfo`, `LinkerError`

### Option 2: Two-Crate Split (Simpler)

#### 1. `lp-riscv-emu` - Emulator + Instruction Utilities

**Purpose:** Everything except ELF loading

**Contents:**

- Everything from Option 1 `lp-riscv-emu`
- Everything from Option 1 `lp-riscv-inst`

**Rationale:** Instruction utilities are small and tightly coupled to emulator

#### 2. `lp-riscv-elf` - ELF Tooling

**Purpose:** Same as Option 1

### Option 3: Rename Only (Not Recommended)

**Just rename `lp-riscv-tools` → `lp-riscv-emu`**

**Problems:**

- Doesn't solve the mixed responsibility issue
- ELF loading still in same crate
- Still confusing feature gating

## Recommendation: Option 1 (Three-Crate Split)

### Benefits

1. **Clear separation of concerns**
    - Runtime emulator vs development tooling
    - Instruction utilities can be used independently

2. **Better dependency management**
    - Consumers only pull what they need
    - `fw-emu` doesn't need ELF loading dependencies

3. **Cleaner feature gating**
    - `lp-riscv-emu` can have optional `std` for time/debug
    - `lp-riscv-elf` requires `std` (no feature flag needed)

4. **Better naming**
    - `lp-riscv-emu` clearly indicates emulator
    - `lp-riscv-inst` clearly indicates instruction utilities
    - `lp-riscv-elf` clearly indicates ELF tooling

### Migration Path

1. **Create new crates:**
    - `lp-riscv/lp-riscv-emu/`
    - `lp-riscv/lp-riscv-inst/`
    - `lp-riscv/lp-riscv-elf/`

2. **Move code:**
    - Move emulator code to `lp-riscv-emu`
    - Move instruction utilities to `lp-riscv-inst`
    - Move ELF loading to `lp-riscv-elf`

3. **Update dependencies:**
    - `lp-riscv-emu` depends on `lp-riscv-inst`
    - `lp-riscv-elf` depends on `lp-riscv-inst` and `lp-riscv-emu`

4. **Update consumers:**
    - `fw-emu`: Use `lp-riscv-emu` only
    - `lp-glsl-compiler`: Use `lp-riscv-emu` and `lp-riscv-elf`

5. **Deprecate `lp-riscv-tools`:**
    - Keep as thin wrapper re-exporting from new crates
    - Add deprecation notice
    - Remove after migration period

## File-by-File Analysis

### Files that go to `lp-riscv-emu`:

- `src/emu/` (all)
- `src/serial/` (all)
- `src/lib.rs` (main emulator exports)

### Files that go to `lp-riscv-inst`:

- `src/decode.rs`
- `src/decode_rvc.rs`
- `src/encode.rs`
- `src/inst.rs`
- `src/format.rs`
- `src/regs.rs`
- `src/register_role.rs`
- `src/auipc_imm.rs`
- `src/debug.rs` (conditional std macro)

### Files that go to `lp-riscv-elf`:

- `src/elf_loader/` (all)
- `src/elf_linker.rs`

## Dependencies Analysis

### Current Dependencies (`lp-riscv-tools`):

```toml
hashbrown = { workspace = true }
nom = { version = "7", default-features = false, features = ["alloc"] }
lp-riscv-emu-shared = { path = "../lp-riscv-emu-shared" }
cranelift-codegen = { workspace = true, features = ["riscv32"] }
cranelift-frontend = { workspace = true, optional = true }
elf = { version = "0.7", optional = true }
object = { version = "0.37.3", optional = true, features = ["write"] }
```

### Proposed Dependencies:

**`lp-riscv-inst`:**

```toml
[dependencies]
# Only alloc for String in error messages
```

**`lp-riscv-emu`:**

```toml
[dependencies]
lp-riscv-inst = { path = "../lp-riscv-inst" }
lp-riscv-emu-shared = { path = "../lp-riscv-emu-shared" }
cranelift-codegen = { workspace = true, features = ["riscv32"] }
hashbrown = { workspace = true }

[features]
std = []  # For time tracking and debug output
```

**`lp-riscv-elf`:**

```toml
[dependencies]
lp-riscv-inst = { path = "../lp-riscv-inst" }
lp-riscv-emu = { path = "../lp-riscv-emu", features = ["std"] }
object = { version = "0.37.3", features = ["write"] }
hashbrown = { workspace = true }
```

## Testing Strategy

### Tests to Move:

- **`lp-riscv-emu`:**
    - `abi_tests.rs`
    - `stack_args_tests.rs`
    - `multi_return_test.rs`
    - `trap_tests.rs`
    - `riscv_nostd_test.rs` (verify no_std works)

- **`lp-riscv-inst`:**
    - `instruction_tests.rs`

- **`lp-riscv-elf`:**
    - `elf_loader_test.rs`
    - `guest_app_tests.rs` (uses ELF loading)

## Open Questions

1. **Should `lp-riscv-inst` depend on `cranelift-codegen`?**
    - Currently instruction utilities don't use it
    - But might be needed for some utilities
    - **Answer:** No, keep it independent

2. **Should `lp-riscv-elf` depend on `lp-riscv-emu`?**
    - ELF loader returns code/ram that goes into emulator
    - But ELF loader itself doesn't need emulator
    - **Answer:** Only if needed for testing, otherwise no

3. **What about `nom` dependency?**
    - Currently used in `lp-riscv-tools` but not clear where
    - **Answer:** Check usage, likely can be removed

4. **Should we keep `lp-riscv-tools` as wrapper?**
    - **Answer:** Yes, temporarily for migration, then remove

## Next Steps

1. **Confirm approach** (Option 1 vs Option 2)
2. **Create migration plan** with step-by-step instructions
3. **Execute migration** following plan
4. **Update all consumers**
5. **Remove deprecated `lp-riscv-tools`**
