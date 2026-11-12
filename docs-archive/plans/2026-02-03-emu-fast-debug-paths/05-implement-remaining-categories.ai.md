# Phase 5: Implement Remaining Category Files

## Scope of Phase

Implement all remaining instruction category files following the pattern established in phase 3. This includes immediate, load/store, branch, jump, system, compressed, and bitmanip categories.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together
- Each instruction gets its own `#[inline]` function

## Implementation Details

### 1. Implement immediate.rs

Create `lp-riscv/lp-riscv-emu/src/emu/executor/immediate.rs` with I-type immediate instructions (ADDI, SLLI, SRLI, SRAI, ANDI, ORI, XORI, SLTI, SLTIU, etc.).

Follow the same pattern as `arithmetic.rs`:
- `decode_execute_itype<M>()` - decodes I-type and routes to instruction functions
- Individual `#[inline]` functions for each instruction (e.g., `execute_addi<M>()`)

### 2. Implement load_store.rs

Create `lp-riscv/lp-riscv-emu/src/emu/executor/load_store.rs` with:
- `decode_execute_load<M>()` - decodes load instructions (LB, LH, LW, LBU, LHU)
- `decode_execute_store<M>()` - decodes store instructions (SB, SH, SW)
- Individual instruction functions for each load/store variant

### 3. Implement branch.rs

Create `lp-riscv/lp-riscv-emu/src/emu/executor/branch.rs` with:
- `decode_execute_branch<M>()` - decodes branch instructions (BEQ, BNE, BLT, BGE, BLTU, BGEU)
- Individual instruction functions for each branch variant

### 4. Implement jump.rs

Create `lp-riscv/lp-riscv-emu/src/emu/executor/jump.rs` with:
- `decode_execute_jal<M>()` - decodes and executes JAL
- `decode_execute_jalr<M>()` - decodes and executes JALR
- Individual instruction functions

### 5. Implement system.rs

Create `lp-riscv/lp-riscv-emu/src/emu/executor/system.rs` with:
- `decode_execute_system<M>()` - decodes system instructions (ECALL, EBREAK, CSR instructions)
- Individual instruction functions for ECALL, EBREAK, and CSR variants

### 6. Implement compressed.rs

Create `lp-riscv/lp-riscv-emu/src/emu/executor/compressed.rs` with:
- `decode_execute_compressed<M>()` - decodes 16-bit compressed instructions
- Routes to appropriate instruction execution functions (reusing functions from other categories where possible)

### 7. Implement bitmanip.rs

Create `lp-riscv/lp-riscv-emu/src/emu/executor/bitmanip.rs` with:
- Bit manipulation instructions (Zbb, Zbs, Zba extensions)
- Follow same pattern as other categories

### 8. Update executor/mod.rs

Uncomment all module declarations and wire up dispatch:

```rust
pub mod arithmetic;
pub mod immediate;
pub mod load_store;
pub mod branch;
pub mod jump;
pub mod system;
pub mod compressed;
pub mod bitmanip;

// Update decode_execute() to route to all categories
```

### 9. Reference: Copy from executor.rs

Use the existing `executor.rs` as a reference for instruction implementations. Copy the logic but adapt it to:
- Use `M::ENABLED` instead of `log_level != LogLevel::None`
- Remove `log_level` parameter
- Use decode-execute fusion (extract fields directly from instruction word)

## Implementation Strategy

1. Start with one category at a time
2. Copy instruction logic from `executor.rs`
3. Adapt to monomorphic pattern
4. Test each category before moving to next
5. Keep `executor.rs` as reference until phase 7

## Tests

For each category, add basic tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_category_fast_path() {
        // Test with LoggingDisabled
    }
    
    #[test]
    fn test_category_logging_path() {
        // Test with LoggingEnabled
    }
}
```

## Validate

After each category:

```bash
cd lp-riscv/lp-riscv-emu
cargo test executor::<category>
cargo check
```

After all categories:

```bash
cargo test
```

Ensure:
- All categories compile
- Fast path has no logging overhead
- Logging path creates InstLog entries
- All tests pass
