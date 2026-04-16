# Phase 2: Refactor step() to use step_inner()

## Scope of phase

Extract the core instruction execution logic into `step_inner()` (without fuel checking), and refactor `step()` to call it. This prepares for the tight loop implementation where fuel checking happens inline.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together

## Implementation Details

### 1. Create step_inner() function

In `lp-riscv/lp-riscv-emu/src/emu/emulator/execution.rs`:

Extract the core logic from `step()` into a new `step_inner()` function:

```rust
impl Riscv32Emulator {
    /// Execute a single instruction (internal, no fuel check).
    /// 
    /// This is the hot path function used by run() loops.
    /// Fuel checking happens in the calling loop, not here.
    #[inline(always)]
    pub(super) fn step_inner(&mut self) -> Result<StepResult, EmulatorError> {
        // Fetch instruction
        let inst_word = self.memory.fetch_instruction(self.pc).map_err(|mut e| {
            // ... error handling ...
        })?;

        // Check if compressed instruction (bits [1:0] != 0b11)
        let is_compressed = (inst_word & 0x3) != 0x3;

        // Decode instruction
        let decoded = decode_instruction(inst_word).map_err(|reason| {
            // ... error handling ...
        })?;

        // Increment instruction count before execution (for cycle counting)
        self.instruction_count += 1;

        // Check if this is a trap BEFORE executing the instruction
        let is_trap_before_execution = if let Inst::Ebreak = decoded {
            self.traps
                .binary_search_by_key(&self.pc, |(addr, _)| *addr)
                .is_ok()
        } else {
            false
        };

        // Execute instruction
        let exec_result = execute_instruction(
            decoded,
            inst_word,
            self.pc,
            &mut self.regs,
            &mut self.memory,
            self.log_level,
        )?;

        // Update PC (2 bytes for compressed, 4 for standard)
        let pc_increment = if is_compressed { 2 } else { 4 };
        self.pc = exec_result
            .new_pc
            .unwrap_or(self.pc.wrapping_add(pc_increment));

        // Log instruction with cycle count (only if logging is enabled)
        if let Some(log) = exec_result.log {
            let log_with_cycle = log.set_cycle(self.instruction_count);
            self.log_instruction(log_with_cycle);
        }

        // Handle special cases (same as current step() logic)
        // ... rest of the logic ...
    }
}
```

### 2. Refactor step() to call step_inner()

Update `step()` to simply call `step_inner()`:

```rust
/// Execute a single instruction.
pub fn step(&mut self) -> Result<StepResult, EmulatorError> {
    // No fuel check - fuel is per-run, not global
    self.step_inner()
}
```

Note: We're removing the fuel check from `step()` since there's no global fuel limit. The old `max_instructions` check will be removed in a later phase.

### 3. Mark hot path functions with #[inline(always)]

Ensure these functions are marked for inlining:
- `step_inner()` - `#[inline(always)]`
- `fetch_instruction()` - Check if already marked, add if not
- `decode_instruction()` - Check if already marked, add if not

## Tests

All existing tests should continue to pass since `step()` maintains the same API and behavior (just without fuel checking, which will be handled differently).

Run existing tests:
```bash
cd lp-riscv/lp-riscv-emu
cargo test
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
- All existing tests pass
- `step_inner()` is marked `#[inline(always)]`
- No performance regressions (we're just refactoring, not changing behavior)
