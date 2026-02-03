# Phase 7: Refactor host_debug to host_log

## Scope of phase

Refactor `__host_debug` to `__host_log` in `lp-glsl-builtins` and update all implementations (emulator, JIT, tests) to use the new signature with level support.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update Function Declaration

**File**: `lp-glsl/lp-glsl-builtins/src/host/mod.rs`

Update comments and exports to reference `__host_log` instead of `__host_debug`.

### 2. Update Emulator Implementation

**File**: `lp-glsl/lp-glsl-builtins-emu-app/src/main.rs` (or wherever `__host_debug` is implemented)

Replace `__host_debug` with `__host_log`:

```rust
/// Log function implementation for emulator.
///
/// This function is called by the builtins logger.
/// Uses SYSCALL_LOG syscall.
#[unsafe(no_mangle)]
pub extern "C" fn __host_log(
    level: u8,
    module_path_ptr: *const u8,
    module_path_len: usize,
    msg_ptr: *const u8,
    msg_len: usize,
) {
    // Convert to syscall format
    let level_i32 = level as i32;
    let module_path_ptr_i32 = module_path_ptr as usize as i32;
    let module_path_len_i32 = module_path_len as i32;
    let msg_ptr_i32 = msg_ptr as usize as i32;
    let msg_len_i32 = msg_len as i32;

    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = level_i32;
    args[1] = module_path_ptr_i32;
    args[2] = module_path_len_i32;
    args[3] = msg_ptr_i32;
    args[4] = msg_len_i32;
    let _ = syscall(SYSCALL_LOG, &args);
}
```

### 3. Update JIT Implementation

**File**: `lp-glsl/lp-glsl-compiler/src/backend/host/impls.rs`

Replace `__host_debug` with `__host_log`:

```rust
/// Log function implementation for JIT mode.
///
/// Creates a log::Record and calls log::log!() directly.
#[unsafe(no_mangle)]
pub extern "C" fn __host_log(
    level: u8,
    module_path_ptr: *const u8,
    module_path_len: usize,
    msg_ptr: *const u8,
    msg_len: usize,
) {
    unsafe {
        // Read strings from pointers
        let module_path_slice = core::slice::from_raw_parts(module_path_ptr, module_path_len);
        let msg_slice = core::slice::from_raw_parts(msg_ptr, msg_len);
        
        let module_path = core::str::from_utf8_unchecked(module_path_slice);
        let msg = core::str::from_utf8_unchecked(msg_slice);

        // Convert level
        let level = match level {
            0 => log::Level::Error,
            1 => log::Level::Warn,
            2 => log::Level::Info,
            3 => log::Level::Debug,
            _ => log::Level::Debug,
        };

        // Create log record and call log::log!()
        log::log!(target: module_path, level, "{}", msg);
    }
}
```

### 4. Update Test Implementation

**File**: `lp-glsl/lp-glsl-builtins/src/host/test.rs`

Replace `__host_debug` with `__host_log`:

```rust
/// Log function implementation for tests.
///
/// Uses log crate directly.
#[cfg(feature = "test")]
pub extern "C" fn __host_log(
    level: u8,
    module_path_ptr: *const u8,
    module_path_len: usize,
    msg_ptr: *const u8,
    msg_len: usize,
) {
    unsafe {
        let module_path_slice = core::slice::from_raw_parts(module_path_ptr, module_path_len);
        let msg_slice = core::slice::from_raw_parts(msg_ptr, msg_len);
        
        let module_path = core::str::from_utf8_unchecked(module_path_slice);
        let msg = core::str::from_utf8_unchecked(msg_slice);

        let level = match level {
            0 => log::Level::Error,
            1 => log::Level::Warn,
            2 => log::Level::Info,
            3 => log::Level::Debug,
            _ => log::Level::Debug,
        };

        log::log!(target: module_path, level, "{}", msg);
    }
}
```

### 5. Update Registry

**File**: `lp-glsl/lp-glsl-builtins/src/host/registry.rs`

Update `HostId` enum if it references `Debug`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostId {
    Log,  // Changed from Debug
    // ... other variants
}
```

Update string mappings:

```rust
impl HostId {
    pub fn name(&self) -> &'static str {
        match self {
            HostId::Log => "__host_log",  // Changed from __host_debug
            // ... other mappings
        }
    }
}
```

### 6. Update JIT Registry

**File**: `lp-glsl/lp-glsl-compiler/src/backend/host/registry.rs`

Update to use `__host_log` instead of `__host_debug`.

## Tests

Update existing tests that use `__host_debug` to use `__host_log` with level parameter.

## Validate

Run from workspace root:

```bash
cargo check --workspace
cargo test --package lp-glsl-builtins --features test
```

Ensure:
- All code compiles
- Function signatures match
- No references to old `__host_debug` remain (except in comments)
