# Phase 4: Implement run() and run_fuel() public API

## Scope of phase

Implement the public `run()` and `run_fuel()` functions that wrap `run_inner()` with error handling and provide the main API for running the emulator.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together

## Implementation Details

### 1. Define default fuel constant

In `lp-riscv/lp-riscv-emu/src/emu/emulator/run_loops.rs`:

```rust
/// Default fuel for run() function
const DEFAULT_FUEL: u64 = 100_000;
```

### 2. Implement run() function

```rust
impl Riscv32Emulator {
    /// Run the emulator with default fuel until yield, halt, trap, panic, or fuel exhaustion.
    ///
    /// Uses default fuel (100_000 instructions). For custom fuel, use `run_fuel()`.
    ///
    /// # Returns
    /// * `Ok(StepResult::Syscall(info))` - Yield syscall encountered (SYSCALL_YIELD)
    /// * `Ok(StepResult::Halted)` - EBREAK encountered (not a trap)
    /// * `Ok(StepResult::Trap(code))` - Trap encountered
    /// * `Ok(StepResult::Panic(info))` - Panic occurred
    /// * `Ok(StepResult::FuelExhausted(count))` - Fuel exhausted (instructions executed)
    /// * `Err(EmulatorError)` - Error occurred (memory access violation, etc.)
    pub fn run(&mut self) -> Result<StepResult, EmulatorError> {
        self.run_fuel(DEFAULT_FUEL)
    }

    /// Run the emulator with specified fuel until yield, halt, trap, panic, or fuel exhaustion.
    ///
    /// # Arguments
    /// * `fuel` - Maximum number of instructions to execute before returning FuelExhausted
    ///
    /// # Returns
    /// * `Ok(StepResult::Syscall(info))` - Yield syscall encountered (SYSCALL_YIELD)
    /// * `Ok(StepResult::Halted)` - EBREAK encountered (not a trap)
    /// * `Ok(StepResult::Trap(code))` - Trap encountered
    /// * `Ok(StepResult::Panic(info))` - Panic occurred
    /// * `Ok(StepResult::FuelExhausted(count))` - Fuel exhausted (instructions executed)
    /// * `Err(EmulatorError)` - Error occurred (memory access violation, etc.)
    pub fn run_fuel(&mut self, fuel: u64) -> Result<StepResult, EmulatorError> {
        self.run_inner(fuel)
    }
}
```

### 3. Update documentation

Ensure the module-level documentation explains the new API and fuel concept.

## Tests

Add tests for `run()` and `run_fuel()`:

```rust
#[test]
fn test_run_default_fuel() {
    let code = vec![0x13, 0x00, 0x00, 0x00]; // nop instruction
    let ram = vec![];
    let mut emu = Riscv32Emulator::new(code, ram);
    
    let initial_count = emu.get_instruction_count();
    let result = emu.run();
    
    // Should exhaust fuel (default is 100_000)
    assert!(matches!(result, Ok(StepResult::FuelExhausted(_))));
    if let Ok(StepResult::FuelExhausted(count)) = result {
        assert_eq!(count, 100_000);
        assert_eq!(emu.get_instruction_count(), initial_count + 100_000);
    }
}

#[test]
fn test_run_fuel_custom() {
    let code = vec![0x13, 0x00, 0x00, 0x00]; // nop instruction
    let ram = vec![];
    let mut emu = Riscv32Emulator::new(code, ram);
    
    let result = emu.run_fuel(10);
    assert!(matches!(result, Ok(StepResult::FuelExhausted(10))));
}

#[test]
fn test_run_until_yield() {
    // Create code that does SYSCALL_YIELD
    // This test will need actual firmware code or a way to inject syscalls
    // For now, just verify the API works
}
```

## Validate

Run:
```bash
cd lp-riscv/lp-riscv-emu
cargo check
cargo test
```

Ensure:
- Code compiles
- Tests pass
- `run()` and `run_fuel()` are public and documented
- Default fuel is 100_000
