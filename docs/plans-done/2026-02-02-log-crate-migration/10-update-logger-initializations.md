# Phase 10: Update Logger Initializations

## Scope of phase

Ensure all entry points (fw-emu, fw-esp32, std applications) properly initialize their loggers. Update any existing initialization code to use the new logger infrastructure.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update fw-emu Initialization

**File**: `lp-fw/fw-emu/src/main.rs`

Ensure logger is initialized (should already be done in Phase 9):

```rust
use fw_core::log::init_emu_logger;

#[unsafe(no_mangle)]
pub extern "C" fn _lp_main() -> ! {
    // Initialize logger first
    init_emu_logger();
    
    // ... rest of code ...
}
```

### 2. Update fw-esp32 Initialization

**File**: `lp-fw/fw-esp32/src/main.rs`

Initialize ESP32 logger:

```rust
use fw_core::log::init_esp32_logger;

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) {
    // Initialize logger
    init_esp32_logger(|s| {
        esp_println::println!("{}", s);
    });

    // Remove old esp_println::logger::init_logger_from_env() if present
    
    // ... rest of code ...
}
```

### 3. Update Std Applications

**Files**: CLI, filetest runner, etc.

Ensure they initialize `env_logger`:

**Example for CLI** (`lp-cli/src/main.rs`):

```rust
fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();
    
    // ... rest of main ...
}
```

**Example for filetest runner** (`lp-glsl/lp-glsl-filetests/src/lib.rs` or main):

```rust
pub fn run_tests() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .init();
    
    // ... rest of code ...
}
```

### 4. Update Emulator Host Initialization

**File**: `lp-riscv/lp-riscv-emu/src/lib.rs` or main entry point

Ensure `env_logger` is initialized before running guest code:

```rust
pub fn run_emulator(...) {
    // Initialize logging first
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();
    
    // ... run guest code ...
}
```

### 5. Update GLSL Builtins Initialization

**File**: `lp-glsl/lp-glsl-compiler/src/exec/executable.rs` or wherever GLSL code runs

Initialize builtins logger before running GLSL code:

```rust
use lp_glsl_builtins::host::init_logger;

// Before running GLSL code:
init_logger();
```

### 6. Remove Old Initialization Code

Search for and remove:
- `esp_println::logger::init_logger_from_env()` calls (replace with our logger)
- Any other old logger initialization code

## Tests

Verify that logging works in each environment:
- Run fw-emu and verify logs appear
- Run fw-esp32 and verify logs appear (if possible)
- Run CLI with `RUST_LOG=debug` and verify logs appear
- Run tests with `RUST_LOG=debug` and verify logs appear

## Validate

Run from workspace root:

```bash
cargo check --workspace
cargo build --package fw-emu
cargo build --package fw-esp32
```

Ensure:
- All code compiles
- All entry points initialize loggers
- No old initialization code remains
- Logging works in each environment
