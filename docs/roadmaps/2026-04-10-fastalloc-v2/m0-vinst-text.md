# M0: VInst Textual IR

## Scope of Work

Create textual representation and parser for VInst. This enables writing expect-style tests directly against VInst sequences without going through GLSL/LPIR parsing.

## Files

```
lp-shader/lpvm-native/src/
└── debug/
    ├── mod.rs                 # NEW: debug module root
    ├── vinst.rs               # NEW: VInst Display + parser
    └── ...
```

## Implementation Details

### 1. Create `debug/mod.rs`

```rust
//! Debug formatting and parsing for IR stages.
//!
//! This module provides textual representations of all IR stages
//! for debugging and testing. All formatting is in forward order
//! (even when the allocator walks backward).

pub mod vinst;
```

### 2. Implement VInst Display in `debug/vinst.rs`

```rust
//! VInst text format and parser.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use crate::vinst::{VInst, VReg, IcmpCond};

/// Format VInsts for human-readable output.
pub fn format_vinsts(vinsts: &[VInst]) -> String {
    let mut lines = Vec::new();
    for (i, inst) in vinsts.iter().enumerate() {
        lines.push(format!("{:4} | {}", i, fmt_vinst(inst)));
    }
    lines.join("\n")
}

fn fmt_vinst(inst: &VInst) -> String {
    match inst {
        VInst::IConst32 { dst, val, .. } => {
            format!("v{} = IConst32 {}", dst.0, val)
        }
        VInst::Add32 { dst, src1, src2, .. } => {
            format!("v{} = Add32 v{}, v{}", dst.0, src1.0, src2.0)
        }
        VInst::Sub32 { dst, src1, src2, .. } => {
            format!("v{} = Sub32 v{}, v{}", dst.0, src1.0, src2.0)
        }
        // ... all other variants
        VInst::Call { args, rets, target, .. } => {
            let args_str = args.iter().map(|v| format!("v{}", v.0)).collect::<Vec<_>>().join(", ");
            if rets.is_empty() {
                format!("Call {} [{}]", target.name, args_str)
            } else {
                let rets_str = rets.iter().map(|v| format!("v{}", v.0)).collect::<Vec<_>>().join(", ");
                format!("v{} = Call {} [{}]", rets_str, target.name, args_str)
            }
        }
        VInst::Ret { vals, .. } => {
            let vals_str = vals.iter().map(|v| format!("v{}", v.0)).collect::<Vec<_>>().join(", ");
            format!("Ret {}", vals_str)
        }
        VInst::Label(id, _) => format!("Label {}:", id),
        VInst::Br { target, .. } => format!("Br {}", target),
        VInst::BrIf { cond, target, invert, .. } => {
            let cond_str = if *invert { "== 0" } else { "!= 0" };
            format!("BrIf v{} {} -> {}", cond.0, cond_str, target)
        }
        // ... all other variants
    }
}
```

### 3. Implement VInst Parser

```rust
#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

/// Parse VInsts from text representation.
pub fn parse_vinsts(input: &str) -> Result<Vec<VInst>, ParseError> {
    let mut vinsts = Vec::new();
    for (line_num, line) in input.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let inst = parse_vinst(line, line_num)?;
        vinsts.push(inst);
    }
    Ok(vinsts)
}

fn parse_vinst(line: &str, line_num: usize) -> Result<VInst, ParseError> {
    // Simple recursive descent parser
    // v0 = IConst32 1
    // v1 = Add32 v0, v2
    // Ret v1

    // Split on " = " for dst
    if let Some((lhs, rhs)) = line.split_once(" = ") {
        let dst = parse_vreg(lhs, line_num)?;
        return parse_def_instruction(dst, rhs.trim(), line_num);
    }

    // No destination: Ret, Br, Label, Call without ret
    parse_nodef_instruction(line, line_num)
}

fn parse_vreg(s: &str, line_num: usize) -> Result<VReg, ParseError> {
    if !s.starts_with("v") {
        return Err(ParseError {
            line: line_num,
            message: format!("Expected vreg like 'v0', got '{}'", s),
        });
    }
    let num: u32 = s[1..].parse().map_err(|_| ParseError {
        line: line_num,
        message: format!("Invalid vreg number in '{}'", s),
    })?;
    Ok(VReg(num))
}

fn parse_def_instruction(dst: VReg, rhs: &str, line_num: usize) -> Result<VInst, ParseError> {
    let parts: Vec<&str> = rhs.split_whitespace().collect();
    match parts.as_slice() {
        &["IConst32", val] => {
            let val: i32 = val.parse().map_err(|_| ParseError {
                line: line_num,
                message: format!("Invalid i32 constant: {}", val),
            })?;
            Ok(VInst::IConst32 { dst, val, src_op: None })
        }
        &["Add32", src1, src2] => {
            let s1 = parse_vreg(src1, line_num)?;
            let s2 = parse_vreg(src2, line_num)?;
            Ok(VInst::Add32 { dst, src1: s1, src2: s2, src_op: None })
        }
        // ... all other variants
        _ => Err(ParseError {
            line: line_num,
            message: format!("Unknown instruction: {}", rhs),
        }),
    }
}

fn parse_nodef_instruction(line: &str, line_num: usize) -> Result<VInst, ParseError> {
    if line.starts_with("Ret ") {
        let rest = &line[4..];
        let vals: Result<Vec<VReg>, _> = rest
            .split(",")
            .map(|s| parse_vreg(s.trim(), line_num))
            .collect();
        Ok(VInst::Ret { vals: vals?, src_op: None })
    } else if line == "Ret" {
        Ok(VInst::Ret { vals: vec![], src_op: None })
    }
    // ... Br, BrIf, Label, Call without ret
    else {
        Err(ParseError {
            line: line_num,
            message: format!("Unknown instruction: {}", line),
        })
    }
}
```

### 4. Add to lp-cli `shader-lpir` command

Update `lp-cli/src/commands/shader_lpir/handler.rs` to add `--show-vinst` flag:

```rust
if args.show_vinst {
    let vinsts = lower_ops(&func)?;
    println!("=== VInst ===");
    println!("{}", lpvm_native::debug::vinst::format_vinsts(&vinsts));
}
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_iconst() {
        let input = "v0 = IConst32 42";
        let vinsts = parse_vinsts(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        let output = format_vinsts(&vinsts);
        assert!(output.contains("IConst32 42"));
    }

    #[test]
    fn test_roundtrip_add() {
        let input = "v0 = Add32 v1, v2";
        let vinsts = parse_vinsts(input).unwrap();
        let output = format_vinsts(&vinsts);
        assert!(output.contains("Add32 v1, v2"));
    }

    #[test]
    fn test_roundtrip_sequence() {
        let input = r#"
            v0 = IConst32 1
            v1 = IConst32 2
            v2 = Add32 v0, v1
            Ret v2
        "#;
        let vinsts = parse_vinsts(input).unwrap();
        assert_eq!(vinsts.len(), 4);

        let output = format_vinsts(&vinsts);
        let reparsed = parse_vinsts(&output).unwrap();
        assert_eq!(vinsts.len(), reparsed.len());
    }
}
```

## Validate

```bash
cd lp-shader/lpvm-native
cargo test -p lpvm-native --lib -- debug::vinst

# Try the CLI
cd lp-cli
cargo run -- shader-lpir some.glsl --show-vinst
```
