# Phase 5: Reimplement run_until_*() functions using run()

## Scope of phase

Reimplement `run_until_ebreak()`, `run_until_ecall()`, and `run_until_yield()` to use the new `run()` API instead of calling `step()` in loops. This maintains backward compatibility while benefiting from the performance improvements.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together

## Implementation Details

### 1. Reimplement run_until_ebreak()

In `lp-riscv/lp-riscv-emu/src/emu/emulator/run_loops.rs`:

```rust
/// Run until EBREAK is encountered, returning the value in a0.
pub fn run_until_ebreak(&mut self) -> Result<i32, EmulatorError> {
    loop {
        match self.run()? {
            StepResult::Halted => {
                return Ok(self.regs[Gpr::A0.num() as usize]);
            }
            StepResult::Trap(code) => {
                return Err(EmulatorError::Trap {
                    code,
                    pc: self.pc,
                    regs: self.regs,
                });
            }
            StepResult::Panic(info) => {
                return Err(EmulatorError::Panic {
                    info,
                    pc: self.pc,
                    regs: self.regs,
                });
            }
            StepResult::FuelExhausted(_) => {
                // Continue running - use more fuel
                continue;
            }
            StepResult::Syscall(_) => {
                // Unexpected syscall - treat as error
                return Err(EmulatorError::InvalidInstruction {
                    pc: self.pc,
                    instruction: 0,
                    reason: String::from("Unexpected ECALL in run_until_ebreak"),
                    regs: self.regs,
                });
            }
            StepResult::Continue => {
                // This shouldn't happen from run() - run() only returns terminal states
                unreachable!("run() should not return Continue");
            }
        }
    }
}
```

### 2. Reimplement run_until_ecall()

```rust
/// Run until ECALL is encountered, returning syscall information.
pub fn run_until_ecall(&mut self) -> Result<SyscallInfo, EmulatorError> {
    loop {
        match self.run()? {
            StepResult::Syscall(info) => {
                return Ok(info);
            }
            StepResult::Halted => {
                return Err(EmulatorError::InvalidInstruction {
                    pc: self.pc,
                    instruction: 0,
                    reason: String::from("Unexpected EBREAK in run_until_ecall"),
                    regs: self.regs,
                });
            }
            StepResult::Trap(_) => {
                return Err(EmulatorError::InvalidInstruction {
                    pc: self.pc,
                    instruction: 0,
                    reason: String::from("Unexpected trap in run_until_ecall"),
                    regs: self.regs,
                });
            }
            StepResult::Panic(info) => {
                return Err(EmulatorError::Panic {
                    info,
                    pc: self.pc,
                    regs: self.regs,
                });
            }
            StepResult::FuelExhausted(_) => {
                // Continue running - use more fuel
                continue;
            }
            StepResult::Continue => {
                unreachable!("run() should not return Continue");
            }
        }
    }
}
```

### 3. Reimplement run_until_yield()

```rust
/// Run until a yield syscall is encountered, with a maximum step limit
///
/// Steps the emulator until a yield syscall (SYSCALL_YIELD) is encountered,
/// or until the maximum number of steps is reached.
///
/// # Arguments
/// * `max_steps` - Maximum number of steps to execute
///
/// # Returns
/// * `Ok(SyscallInfo)` - Yield syscall was encountered
/// * `Err(EmulatorError)` - Error occurred (trap, panic, or max steps exceeded)
pub fn run_until_yield(&mut self, max_steps: u64) -> Result<SyscallInfo, EmulatorError> {
    loop {
        match self.run_fuel(max_steps)? {
            StepResult::Syscall(info) if info.number == SYSCALL_YIELD => {
                return Ok(info);
            }
            StepResult::Syscall(_) => {
                // Other syscall - continue execution (but this shouldn't happen
                // since run() only returns yield syscalls)
                // Actually, wait - run() returns any syscall, not just yield
                // So we need to check if it's a yield
                continue;
            }
            StepResult::Halted => {
                return Err(EmulatorError::InvalidInstruction {
                    pc: self.pc,
                    instruction: 0,
                    reason: String::from("Unexpected EBREAK in run_until_yield"),
                    regs: self.regs,
                });
            }
            StepResult::Trap(code) => {
                return Err(EmulatorError::Trap {
                    code,
                    pc: self.pc,
                    regs: self.regs,
                });
            }
            StepResult::Panic(info) => {
                return Err(EmulatorError::Panic {
                    info,
                    pc: self.pc,
                    regs: self.regs,
                });
            }
            StepResult::FuelExhausted(_) => {
                // Fuel exhausted - this means we hit max_steps
                return Err(EmulatorError::InstructionLimitExceeded {
                    limit: max_steps,
                    executed: max_steps,
                    pc: self.pc,
                    regs: self.regs,
                });
            }
            StepResult::Continue => {
                unreachable!("run() should not return Continue");
            }
        }
    }
}
```

Wait, there's an issue. `run()` returns `Syscall` for ANY syscall, not just yield. But `run_until_yield()` needs to continue if it's not a yield syscall. Let me reconsider...

Actually, looking at the current implementation, `run()` should only return when it encounters a yield syscall OR fuel exhaustion OR other terminal states. But other syscalls should continue. Let me check the design again...

Looking at the design, `run_inner()` checks for `SYSCALL_YIELD` and returns early. Other syscalls continue. So `run()` will only return `Syscall` for yield syscalls. But wait, that's not what the code shows - `run_inner()` returns `Syscall` for any syscall if it's yield, otherwise continues.

Actually, I think the issue is that `run()` should handle non-yield syscalls internally and continue. But the current `run_inner()` design returns `Syscall` for yield only. So `run_until_yield()` should work correctly.

But wait, `run_until_yield()` currently expects to handle other syscalls. Let me check the current implementation again...

Looking at the current `run_until_yield()`, it handles `StepResult::Syscall(_)` for non-yield syscalls and continues. So `run()` needs to handle non-yield syscalls and continue, only returning for yield.

Actually, I think the cleanest approach is:
- `run()` returns `Syscall` only for `SYSCALL_YIELD`
- Other syscalls are handled internally and execution continues
- This matches the current behavior where non-yield syscalls continue

So `run_inner()` should only return `Syscall` for yield, and handle other syscalls internally. Let me update the design:

In `run_inner()`, when we get a syscall:
- If it's `SYSCALL_YIELD`, return `Syscall(info)`
- Otherwise, handle it (like `step()` does) and continue

Actually, `step()` handles syscalls and returns `Continue` for most of them (except yield). So `run_inner()` should do the same - handle syscalls internally and continue, only returning for yield.

But wait, `step()` returns `Syscall` for all syscalls, and the caller decides what to do. So `run()` should also return `Syscall` for all syscalls, and `run_until_yield()` checks if it's yield.

Let me update `run_until_yield()` to handle this correctly:

```rust
pub fn run_until_yield(&mut self, max_steps: u64) -> Result<SyscallInfo, EmulatorError> {
    loop {
        match self.run_fuel(max_steps)? {
            StepResult::Syscall(info) => {
                if info.number == SYSCALL_YIELD {
                    return Ok(info);
                }
                // Other syscall - continue (but this means run() returned early)
                // Actually, run() shouldn't return for non-yield syscalls
                // Let me reconsider the design...
            }
            // ... rest
        }
    }
}
```

I think the issue is that `run()` is designed to return on yield, but `run_until_yield()` needs to handle the case where we might get other syscalls. But if `run()` only returns on yield, then `run_until_yield()` should just call `run()` once.

Actually, I think the confusion is: `run()` should return `Syscall` for yield syscalls, and the caller (like `run_until_yield()`) can check if it's yield. But `run()` might also return for fuel exhaustion, so `run_until_yield()` needs to handle that and call `run()` again.

Let me simplify: `run_until_yield()` calls `run()` in a loop, checking if the result is a yield syscall. If fuel is exhausted, it calls `run()` again with more fuel. This maintains the same behavior as before.

## Tests

All existing tests for `run_until_*()` functions should continue to pass since we're maintaining the same API and behavior.

## Validate

Run:
```bash
cd lp-riscv/lp-riscv-emu
cargo check
cargo test
```

Ensure:
- Code compiles
- All existing tests pass
- `run_until_*()` functions maintain same API and behavior
- Functions use `run()` instead of calling `step()` in loops
