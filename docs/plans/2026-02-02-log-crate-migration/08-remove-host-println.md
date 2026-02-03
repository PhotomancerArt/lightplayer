# Phase 8: Remove host_println

## Scope of phase

Remove `__host_println` function and `host_println!` macro. Update all usages to use appropriate log levels (`log::info!` for what was println).

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Remove host_println! Macro

**File**: `lp-glsl/lp-glsl-builtins/src/host/macros.rs`

Remove the `host_println!` macro definition entirely.

### 2. Remove __host_println Function Declarations

**File**: `lp-glsl/lp-glsl-builtins/src/host/mod.rs`

Remove exports and references to `__host_println`.

### 3. Remove __host_println Implementations

**Files to update**:
- `lp-glsl/lp-glsl-builtins-emu-app/src/main.rs` - Remove emulator implementation
- `lp-glsl/lp-glsl-compiler/src/backend/host/impls.rs` - Remove JIT implementation
- `lp-glsl/lp-glsl-builtins/src/host/test.rs` - Remove test implementation
- `lp-riscv/lp-riscv-emu-guest/src/host.rs` - Remove guest implementation

### 4. Update Registry

**File**: `lp-glsl/lp-glsl-builtins/src/host/registry.rs`

Remove `Println` variant from `HostId` enum if it exists.

**File**: `lp-glsl/lp-glsl-compiler/src/backend/host/registry.rs`

Remove `Println` references from JIT registry.

### 5. Find and Replace Usages

Search for all usages of `host_println!` and replace with appropriate log level:

- `host_println!("message")` → `log::info!("message")`
- `host_println!("format {}", arg)` → `log::info!("format {}", arg)`

**Files to check**:
- `lp-fw/fw-emu/src/main.rs` - Replace `host_debug!` with `log::debug!` if present
- `lp-fw/fw-emu/src/output.rs` - Replace `println!` with `log::info!`
- `lp-fw/fw-emu/src/serial.rs` - Check for any println usage
- Any other files using `host_println!`

### 6. Remove SYSCALL_WRITE Usage

**File**: `lp-riscv/lp-riscv-emu-guest/src/host.rs`

Remove `__host_println` function that uses `SYSCALL_WRITE`. We're moving everything to logging.

**File**: `lp-riscv/lp-riscv-emu/src/emu/emulator/execution.rs`

We can keep `SYSCALL_WRITE` handling for now (it might be used elsewhere), but remove any references to it being used for `host_println`.

### 7. Update ESP32 JIT Functions

**File**: `lp-glsl/esp32-glsl-jit/src/jit_fns.rs`

Remove `lp_jit_host_println` function. Replace usages with logging.

## Tests

Update any tests that use `host_println!` to use `log::info!` instead.

## Validate

Run from workspace root:

```bash
cargo check --workspace
grep -r "host_println" --include="*.rs"
grep -r "__host_println" --include="*.rs"
```

Ensure:
- All code compiles
- No references to `host_println!` or `__host_println` remain (except in comments/TODOs)
- All usages have been replaced with appropriate log levels
