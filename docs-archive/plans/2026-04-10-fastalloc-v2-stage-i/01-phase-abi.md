# Phase 1: ABI and Directory Structure

## Scope

Create `rv32fa/` directory and copy ABI definitions from `rv32/abi.rs`.

## Implementation

### 1. Create directory structure

```bash
mkdir -p lp-shader/lpvm-native/src/isa/rv32fa/debug
```

### 2. Create `rv32fa/mod.rs`

```rust
//! Fast allocator pipeline for RV32.
//!
//! This pipeline replaces the legacy rv32/ pipeline with:
//! - Backward-walk register allocator producing PhysInst
//! - Functional emitter with no indirection
//!
//! Uses standard RISC-V assembly syntax for text representation.

pub mod abi;
pub mod debug;
pub mod inst;
```

### 3. Create `rv32fa/debug/mod.rs`

```rust
//! Debug formatting and parsing for RV32 fastalloc pipeline.

pub mod physinst;
```

### 4. Copy `rv32/abi.rs` to `rv32fa/abi.rs`

Copy the entire file. Key exports:
- `ARG_REGS`, `RET_REGS`
- `FP_REG`, `SP_REG`, `RA_REG`
- `callee_saved_int()`, `caller_saved_int()`
- `reg_name()`
- Add `parse_reg()` for parsing "a0" -> 10

### 5. Add `parse_reg()` function

```rust
/// Parse register name to physical register number.
/// Supports standard RISC-V ABI names.
pub fn parse_reg(name: &str) -> Result<u8, ()> {
    match name {
        // x0-x31
        "x0" | "zero" => Ok(0),
        "x1" | "ra" => Ok(1),
        "x2" | "sp" => Ok(2),
        "x3" | "gp" => Ok(3),
        "x4" | "tp" => Ok(4),
        "x5" | "t0" => Ok(5),
        "x6" | "t1" => Ok(6),
        "x7" | "t2" => Ok(7),
        "x8" | "s0" | "fp" => Ok(8),
        "x9" | "s1" => Ok(9),
        "x10" | "a0" => Ok(10),
        "x11" | "a1" => Ok(11),
        "x12" | "a2" => Ok(12),
        "x13" | "a3" => Ok(13),
        "x14" | "a4" => Ok(14),
        "x15" | "a5" => Ok(15),
        "x16" | "a6" => Ok(16),
        "x17" | "a7" => Ok(17),
        "x18" | "s2" => Ok(18),
        "x19" | "s3" => Ok(19),
        "x20" | "s4" => Ok(20),
        "x21" | "s5" => Ok(21),
        "x22" | "s6" => Ok(22),
        "x23" | "s7" => Ok(23),
        "x24" | "s8" => Ok(24),
        "x25" | "s9" => Ok(25),
        "x26" | "s10" => Ok(26),
        "x27" | "s11" => Ok(27),
        "x28" | "t3" => Ok(28),
        "x29" | "t4" => Ok(29),
        "x30" | "t5" => Ok(30),
        "x31" | "t6" => Ok(31),
        _ => Err(()),
    }
}
```

## Tests

```rust
#[test]
fn test_parse_reg() {
    assert_eq!(parse_reg("a0"), Ok(10));
    assert_eq!(parse_reg("x10"), Ok(10));
    assert_eq!(parse_reg("s0"), Ok(8));
    assert_eq!(parse_reg("fp"), Ok(8));
    assert_eq!(parse_reg("ra"), Ok(1));
}

#[test]
fn test_reg_name_roundtrip() {
    for i in 0..32 {
        let name = reg_name(i);
        assert_eq!(parse_reg(name), Ok(i), "Roundtrip failed for {}", i);
    }
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- rv32fa::abi
cargo check -p lpvm-native --lib
```
