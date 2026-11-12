# Phase 3: Implement run_inner() with tight loop

## Scope of phase

Implement `run_inner()` with a tight loop that checks fuel inline and uses performance optimizations (inline hints, branch prediction hints).

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together

## Implementation Details

### 1. Add run_inner() function

In `lp-riscv/lp-riscv-emu/src/emu/emulator/run_loops.rs`:

```rust
impl Riscv32Emulator {
    /// Internal run loop with tight loop and inline fuel checking.
    /// 
    /// This is the hot path - fuel checking happens inline in the loop
    /// to minimize function call overhead.
    pub(super) fn run_inner(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
        loop {
            // Inline fuel check - decrement and check in the loop
            fuel -= 1;
            if fuel == 0 {
                // Calculate instructions executed in this run
                // We need to track this - maybe use a local counter?
                // Actually, we can't easily know how many were executed without
                // tracking it separately. Let's use the fuel parameter differently.
                // 
                // Better approach: track instructions executed in this run
                let instructions_executed = /* TODO: track this */;
                return Ok(StepResult::FuelExhausted(instructions_executed));
            }

            // Call step_inner() (no fuel check, already checked above)
            match self.step_inner()? {
                StepResult::Continue => {
                    // Most common case - use likely() hint
                    #[cfg(target_arch = "x86_64")]
                    {
                        core::intrinsics::likely(true);
                    }
                    continue;
                }
                StepResult::Syscall(info) => {
                    // Check if this is a yield syscall
                    if info.number == SYSCALL_YIELD {
                        return Ok(StepResult::Syscall(info));
                    }
                    // Other syscall - continue (but this is less common)
                    #[cfg(target_arch = "x86_64")]
                    {
                        core::intrinsics::unlikely(true);
                    }
                    continue;
                }
                StepResult::Halted => {
                    return Ok(StepResult::Halted);
                }
                StepResult::Trap(code) => {
                    return Ok(StepResult::Trap(code));
                }
                StepResult::Panic(info) => {
                    return Ok(StepResult::Panic(info));
                }
                StepResult::FuelExhausted(_) => {
                    // This should never happen from step_inner()
                    unreachable!("step_inner() should never return FuelExhausted");
                }
            }
        }
    }
}
```

Wait, we need to track instructions executed in this run. Let's use a local counter:

```rust
pub(super) fn run_inner(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
    let initial_instruction_count = self.instruction_count;
    
    loop {
        // Inline fuel check - decrement and check in the loop
        fuel -= 1;
        if fuel == 0 {
            let instructions_executed = self.instruction_count - initial_instruction_count;
            return Ok(StepResult::FuelExhausted(instructions_executed));
        }

        // Call step_inner() (no fuel check, already checked above)
        match self.step_inner()? {
            StepResult::Continue => {
                // Most common case - continue execution
                // Note: likely()/unlikely() intrinsics may not be available
                // We can use #[cold] attribute on less common paths instead
                continue;
            }
            StepResult::Syscall(info) => {
                // Check if this is a yield syscall
                if info.number == SYSCALL_YIELD {
                    return Ok(StepResult::Syscall(info));
                }
                // Other syscall - continue
                continue;
            }
            StepResult::Halted => {
                return Ok(StepResult::Halted);
            }
            StepResult::Trap(code) => {
                return Ok(StepResult::Trap(code));
            }
            StepResult::Panic(info) => {
                return Ok(StepResult::Panic(info));
            }
            StepResult::FuelExhausted(_) => {
                unreachable!("step_inner() should never return FuelExhausted");
            }
        }
    }
}
```

Actually, let's check what intrinsics are available. `likely()`/`unlikely()` might not be stable. Let's use `#[cold]` attribute on the return paths instead, which is stable.

### 2. Use #[cold] attribute for less common paths

Mark the return paths (yield, halt, trap, panic) with `#[cold]` to help the optimizer:

```rust
#[cold]
fn handle_yield(info: SyscallInfo) -> StepResult {
    StepResult::Syscall(info)
}
```

Actually, `#[cold]` is for functions, not match arms. Let's keep it simple for now and focus on the tight loop structure. The compiler should optimize the continue path naturally.

### 3. Ensure step_inner() is inlined

Verify that `step_inner()` is marked `#[inline(always)]` (done in phase 2).

## Tests

Add a basic test for `run_inner()`:

```rust
#[test]
fn test_run_inner_fuel_exhausted() {
    let code = vec![0x13, 0x00, 0x00, 0x00]; // nop instruction
    let ram = vec![];
    let mut emu = Riscv32Emulator::new(code, ram);
    
    // Run with small fuel
    let result = emu.run_inner(5);
    assert!(matches!(result, Ok(StepResult::FuelExhausted(5))));
    assert_eq!(emu.get_instruction_count(), 5);
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
- `run_inner()` implements tight loop with inline fuel checking
- `step_inner()` is called (not `step()`)
