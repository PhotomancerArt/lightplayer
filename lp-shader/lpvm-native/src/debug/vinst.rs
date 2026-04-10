//! VInst text format and parser.
//!
//! Format:
//!   i2 = Add32 i0, i1          // binary op
//!   i0 = IConst32 42           // immediate
//!   i3 = Icmp32 Eq, i0, i1     // comparison (cond first)
//!   i3 = Select32 i0, i1, i2   // cond, if_true, if_false
//!   i1 = Load32 i0, 4          // base, offset (optional)
//!   Store32 i1, i0, 4          // src, base, offset
//!   (i2, i3) = Call mod (i0, i1)  // multi-ret and args
//!   Ret i0                     // return values
//!   Br @0                      // branch to label
//!   BrIf i0, @1                // branch if i0 != 0
//!   @0:                        // label definition

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::vinst::{IcmpCond, SymbolRef, VInst, VReg};

/// Parse VInsts from text representation.
pub fn parse(input: &str) -> Result<Vec<VInst>, ParseError> {
    let mut vinsts = Vec::new();
    for (line_num, line) in input.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let inst = parse_line(line, line_num)?;
        vinsts.push(inst);
    }
    Ok(vinsts)
}

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

fn parse_line(line: &str, line_num: usize) -> Result<VInst, ParseError> {
    // Label definition: @0:
    if line.starts_with("@") && line.ends_with(":") {
        let id_str = &line[1..line.len()-1];
        let id: u32 = id_str.parse().map_err(|_| ParseError {
            line: line_num,
            message: format!("Invalid label id: {}", id_str),
        })?;
        return Ok(VInst::Label(id, None));
    }

    // Assignment: i2 = Add32 i0, i1
    if let Some((lhs, rhs)) = line.split_once(" = ") {
        let dsts = parse_rets(lhs.trim())?;
        return parse_def_instruction(dsts, rhs.trim(), line_num);
    }

    // No assignment: Store32, Br, BrIf, Ret, Call without ret
    parse_nodef_instruction(line, line_num)
}

/// Parse return register(s), e.g., "i2" or "(i2, i3)"
fn parse_rets(s: &str) -> Result<Vec<VReg>, ParseError> {
    if s.starts_with("(") && s.ends_with(")") {
        // Multi-ret: (i2, i3)
        let inner = &s[1..s.len()-1];
        let mut regs = Vec::new();
        for part in inner.split(',') {
            regs.push(parse_ireg(part.trim())?);
        }
        Ok(regs)
    } else {
        // Single ret: i2
        Ok(vec![parse_ireg(s)?])
    }
}

/// Parse integer register: i0, i1, etc.
fn parse_ireg(s: &str) -> Result<VReg, ParseError> {
    if !s.starts_with("i") {
        return Err(ParseError {
            line: 0,
            message: format!("Expected ireg like 'i0', got '{}'", s),
        });
    }
    let num: u32 = s[1..].parse().map_err(|_| ParseError {
        line: 0,
        message: format!("Invalid ireg number in '{}'", s),
    })?;
    Ok(VReg(num))
}

/// Parse comma-separated argument list: i0, i1 or i0
fn parse_args(s: &str) -> Result<Vec<VReg>, ParseError> {
    s.split(',').map(|p| parse_ireg(p.trim())).collect()
}

/// Parse arguments in parens: (i0, i1) or empty ()
fn parse_paren_args(s: &str) -> Result<Vec<VReg>, ParseError> {
    if !s.starts_with("(") || !s.ends_with(")") {
        return Err(ParseError {
            line: 0,
            message: format!("Expected parens like '(i0, i1)', got '{}'", s),
        });
    }
    let inner = &s[1..s.len()-1];
    if inner.trim().is_empty() {
        return Ok(vec![]);
    }
    parse_args(inner)
}

/// Parse label reference: @0
fn parse_label(s: &str) -> Result<u32, ParseError> {
    if !s.starts_with("@") {
        return Err(ParseError {
            line: 0,
            message: format!("Expected label like '@0', got '{}'", s),
        });
    }
    s[1..].parse().map_err(|_| ParseError {
        line: 0,
        message: format!("Invalid label id in '{}'", s),
    })
}

fn parse_def_instruction(
    dsts: Vec<VReg>,
    rhs: &str,
    line_num: usize,
) -> Result<VInst, ParseError> {
    let parts: Vec<&str> = rhs.split_whitespace().collect();
    if parts.is_empty() {
        return Err(ParseError {
            line: line_num,
            message: "Empty instruction".into(),
        });
    }

    let op = parts[0];
    let args_str = if parts.len() > 1 {
        parts[1..].join(" ")
    } else {
        String::new()
    };

    match op {
        "IConst32" => {
            if dsts.len() != 1 {
                return Err(ParseError { line: line_num, message: "IConst32 needs 1 dst".into() });
            }
            let val: i32 = args_str.trim().parse().map_err(|_| ParseError {
                line: line_num,
                message: format!("Invalid i32: {}", args_str),
            })?;
            Ok(VInst::IConst32 { dst: dsts[0], val, src_op: None })
        }

        "Add32" | "Sub32" | "Mul32" | "And32" | "Or32" | "Xor32" |
        "Shl32" | "ShrS32" | "ShrU32" => {
            if dsts.len() != 1 {
                return Err(ParseError { line: line_num, message: format!("{} needs 1 dst", op) });
            }
            let args = parse_args(&args_str)?;
            if args.len() != 2 {
                return Err(ParseError { line: line_num, message: format!("{} needs 2 args", op) });
            }
            let (src1, src2) = (args[0], args[1]);
            match op {
                "Add32" => Ok(VInst::Add32 { dst: dsts[0], src1, src2, src_op: None }),
                "Sub32" => Ok(VInst::Sub32 { dst: dsts[0], src1, src2, src_op: None }),
                "Mul32" => Ok(VInst::Mul32 { dst: dsts[0], src1, src2, src_op: None }),
                "And32" => Ok(VInst::And32 { dst: dsts[0], src1, src2, src_op: None }),
                "Or32" => Ok(VInst::Or32 { dst: dsts[0], src1, src2, src_op: None }),
                "Xor32" => Ok(VInst::Xor32 { dst: dsts[0], src1, src2, src_op: None }),
                "Shl32" => Ok(VInst::Shl32 { dst: dsts[0], src1, src2, src_op: None }),
                "ShrS32" => Ok(VInst::ShrS32 { dst: dsts[0], src1, src2, src_op: None }),
                "ShrU32" => Ok(VInst::ShrU32 { dst: dsts[0], src1, src2, src_op: None }),
                _ => unreachable!(),
            }
        }

        "DivS32" | "DivU32" | "RemS32" | "RemU32" => {
            if dsts.len() != 1 {
                return Err(ParseError { line: line_num, message: format!("{} needs 1 dst", op) });
            }
            let args = parse_args(&args_str)?;
            if args.len() != 2 {
                return Err(ParseError { line: line_num, message: format!("{} needs 2 args", op) });
            }
            let (lhs, rhs) = (args[0], args[1]);
            match op {
                "DivS32" => Ok(VInst::DivS32 { dst: dsts[0], lhs, rhs, src_op: None }),
                "DivU32" => Ok(VInst::DivU32 { dst: dsts[0], lhs, rhs, src_op: None }),
                "RemS32" => Ok(VInst::RemS32 { dst: dsts[0], lhs, rhs, src_op: None }),
                "RemU32" => Ok(VInst::RemU32 { dst: dsts[0], lhs, rhs, src_op: None }),
                _ => unreachable!(),
            }
        }

        "Neg32" | "Bnot32" | "Mov32" => {
            if dsts.len() != 1 {
                return Err(ParseError { line: line_num, message: format!("{} needs 1 dst", op) });
            }
            let src = parse_ireg(args_str.trim())?;
            match op {
                "Neg32" => Ok(VInst::Neg32 { dst: dsts[0], src, src_op: None }),
                "Bnot32" => Ok(VInst::Bnot32 { dst: dsts[0], src, src_op: None }),
                "Mov32" => Ok(VInst::Mov32 { dst: dsts[0], src, src_op: None }),
                _ => unreachable!(),
            }
        }

        "Icmp32" => {
            if dsts.len() != 1 {
                return Err(ParseError { line: line_num, message: "Icmp32 needs 1 dst".into() });
            }
            // Format: Eq, i0, i1
            let parts: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
            if parts.len() != 3 {
                return Err(ParseError { line: line_num, message: "Icmp32 needs 'Eq, i0, i1'".into() });
            }
            let cond = parse_icmp_cond(parts[0])?;
            let lhs = parse_ireg(parts[1])?;
            let rhs = parse_ireg(parts[2])?;
            Ok(VInst::Icmp32 { dst: dsts[0], lhs, rhs, cond, src_op: None })
        }

        "IeqImm32" => {
            if dsts.len() != 1 {
                return Err(ParseError { line: line_num, message: "IeqImm32 needs 1 dst".into() });
            }
            let parts: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
            if parts.len() != 2 {
                return Err(ParseError { line: line_num, message: "IeqImm32 needs 'i0, 42'".into() });
            }
            let src = parse_ireg(parts[0])?;
            let imm: i32 = parts[1].parse().map_err(|_| ParseError {
                line: line_num,
                message: format!("Invalid imm: {}", parts[1]),
            })?;
            Ok(VInst::IeqImm32 { dst: dsts[0], src, imm, src_op: None })
        }

        "Select32" => {
            if dsts.len() != 1 {
                return Err(ParseError { line: line_num, message: "Select32 needs 1 dst".into() });
            }
            let args = parse_args(&args_str)?;
            if args.len() != 3 {
                return Err(ParseError { line: line_num, message: "Select32 needs 3 args".into() });
            }
            Ok(VInst::Select32 { dst: dsts[0], cond: args[0], if_true: args[1], if_false: args[2], src_op: None })
        }

        "Load32" => {
            if dsts.len() != 1 {
                return Err(ParseError { line: line_num, message: "Load32 needs 1 dst".into() });
            }
            let parts: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
            let base = parse_ireg(parts[0])?;
            let offset = if parts.len() > 1 {
                parts[1].parse().map_err(|_| ParseError {
                    line: line_num,
                    message: format!("Invalid offset: {}", parts[1]),
                })?
            } else {
                0
            };
            Ok(VInst::Load32 { dst: dsts[0], base, offset, src_op: None })
        }

        "SlotAddr" => {
            if dsts.len() != 1 {
                return Err(ParseError { line: line_num, message: "SlotAddr needs 1 dst".into() });
            }
            let slot: u32 = args_str.trim().parse().map_err(|_| ParseError {
                line: line_num,
                message: format!("Invalid slot: {}", args_str),
            })?;
            Ok(VInst::SlotAddr { dst: dsts[0], slot, src_op: None })
        }

        "Call" => {
            // Format: mod (i0, i1) or just mod for no args
            let open_paren = args_str.find('(').unwrap_or(args_str.len());
            let target_name = args_str[..open_paren].trim();
            let args = if open_paren < args_str.len() {
                parse_paren_args(&args_str[open_paren..])?
            } else {
                vec![]
            };
            Ok(VInst::Call {
                target: SymbolRef { name: target_name.into() },
                args,
                rets: dsts,
                callee_uses_sret: false,  // TODO: detect from rets.len()
                src_op: None,
            })
        }

        _ => Err(ParseError {
            line: line_num,
            message: format!("Unknown instruction: {}", op),
        }),
    }
}

fn parse_nodef_instruction(line: &str, line_num: usize) -> Result<VInst, ParseError> {
    // Ret i0 or Ret (i0, i1) or Ret
    if line.starts_with("Ret ") || line == "Ret" {
        let rest = if line.starts_with("Ret ") {
            &line[4..]
        } else {
            ""
        };
        let vals = if rest.trim().is_empty() {
            vec![]
        } else if rest.starts_with("(") && rest.ends_with(")") {
            parse_paren_args(rest)?
        } else {
            vec![parse_ireg(rest.trim())?]
        };
        return Ok(VInst::Ret { vals, src_op: None });
    }

    // Store32 i1, i0, 4
    if line.starts_with("Store32 ") {
        let rest = &line[8..];
        let parts: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
        if parts.len() < 2 {
            return Err(ParseError { line: line_num, message: "Store32 needs src, base[, offset]".into() });
        }
        let src = parse_ireg(parts[0])?;
        let base = parse_ireg(parts[1])?;
        let offset = if parts.len() > 2 {
            parts[2].parse().map_err(|_| ParseError {
                line: line_num,
                message: format!("Invalid offset: {}", parts[2]),
            })?
        } else {
            0
        };
        return Ok(VInst::Store32 { src, base, offset, src_op: None });
    }

    // MemcpyWords i0, i1, 16
    if line.starts_with("MemcpyWords ") {
        let rest = &line[11..];
        let parts: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
        if parts.len() != 3 {
            return Err(ParseError { line: line_num, message: "MemcpyWords needs dst_base, src_base, size".into() });
        }
        let dst_base = parse_ireg(parts[0])?;
        let src_base = parse_ireg(parts[1])?;
        let size: u32 = parts[2].parse().map_err(|_| ParseError {
            line: line_num,
            message: format!("Invalid size: {}", parts[2]),
        })?;
        return Ok(VInst::MemcpyWords { dst_base, src_base, size, src_op: None });
    }

    // Br @0
    if line.starts_with("Br ") {
        let rest = &line[3..];
        let target = parse_label(rest.trim())?;
        return Ok(VInst::Br { target, src_op: None });
    }

    // BrIf i0, @1
    if line.starts_with("BrIf ") {
        let rest = &line[5..];
        let parts: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
        if parts.len() != 2 {
            return Err(ParseError { line: line_num, message: "BrIf needs cond, @label".into() });
        }
        let cond = parse_ireg(parts[0])?;
        let target = parse_label(parts[1])?;
        return Ok(VInst::BrIf { cond, target, invert: false, src_op: None });
    }

    // Call mod (i0, i1) with no return
    if line.starts_with("Call ") {
        let rest = &line[5..];
        let open_paren = rest.find('(').unwrap_or(rest.len());
        let target_name = rest[..open_paren].trim();
        let args = if open_paren < rest.len() {
            parse_paren_args(&rest[open_paren..])?
        } else {
            vec![]
        };
        return Ok(VInst::Call {
            target: SymbolRef { name: target_name.into() },
            args,
            rets: vec![],
            callee_uses_sret: false,
            src_op: None,
        });
    }

    Err(ParseError {
        line: line_num,
        message: format!("Unknown instruction: {}", line),
    })
}

fn parse_icmp_cond(s: &str) -> Result<IcmpCond, ParseError> {
    match s {
        "Eq" => Ok(IcmpCond::Eq),
        "Ne" => Ok(IcmpCond::Ne),
        "LtS" => Ok(IcmpCond::LtS),
        "LeS" => Ok(IcmpCond::LeS),
        "GtS" => Ok(IcmpCond::GtS),
        "GeS" => Ok(IcmpCond::GeS),
        "LtU" => Ok(IcmpCond::LtU),
        "LeU" => Ok(IcmpCond::LeU),
        "GtU" => Ok(IcmpCond::GtU),
        "GeU" => Ok(IcmpCond::GeU),
        _ => Err(ParseError { line: 0, message: format!("Unknown cond: {}", s) }),
    }
}

/// Format VInsts for human-readable output.
pub fn format(vinsts: &[VInst]) -> String {
    vinsts.iter().map(format_vinst).collect::<Vec<_>>().join("\n")
}

fn format_vinst(inst: &VInst) -> String {
    match inst {
        VInst::Label(id, _) => format!("@{}:", id),

        VInst::IConst32 { dst, val, .. } => {
            format!("{} = IConst32 {}", ireg(dst), val)
        }

        VInst::Add32 { dst, src1, src2, .. } => {
            format!("{} = Add32 {}, {}", ireg(dst), ireg(src1), ireg(src2))
        }
        VInst::Sub32 { dst, src1, src2, .. } => {
            format!("{} = Sub32 {}, {}", ireg(dst), ireg(src1), ireg(src2))
        }
        VInst::Mul32 { dst, src1, src2, .. } => {
            format!("{} = Mul32 {}, {}", ireg(dst), ireg(src1), ireg(src2))
        }
        VInst::And32 { dst, src1, src2, .. } => {
            format!("{} = And32 {}, {}", ireg(dst), ireg(src1), ireg(src2))
        }
        VInst::Or32 { dst, src1, src2, .. } => {
            format!("{} = Or32 {}, {}", ireg(dst), ireg(src1), ireg(src2))
        }
        VInst::Xor32 { dst, src1, src2, .. } => {
            format!("{} = Xor32 {}, {}", ireg(dst), ireg(src1), ireg(src2))
        }
        VInst::Shl32 { dst, src1, src2, .. } => {
            format!("{} = Shl32 {}, {}", ireg(dst), ireg(src1), ireg(src2))
        }
        VInst::ShrS32 { dst, src1, src2, .. } => {
            format!("{} = ShrS32 {}, {}", ireg(dst), ireg(src1), ireg(src2))
        }
        VInst::ShrU32 { dst, src1, src2, .. } => {
            format!("{} = ShrU32 {}, {}", ireg(dst), ireg(src1), ireg(src2))
        }

        VInst::DivS32 { dst, lhs, rhs, .. } => {
            format!("{} = DivS32 {}, {}", ireg(dst), ireg(lhs), ireg(rhs))
        }
        VInst::DivU32 { dst, lhs, rhs, .. } => {
            format!("{} = DivU32 {}, {}", ireg(dst), ireg(lhs), ireg(rhs))
        }
        VInst::RemS32 { dst, lhs, rhs, .. } => {
            format!("{} = RemS32 {}, {}", ireg(dst), ireg(lhs), ireg(rhs))
        }
        VInst::RemU32 { dst, lhs, rhs, .. } => {
            format!("{} = RemU32 {}, {}", ireg(dst), ireg(lhs), ireg(rhs))
        }

        VInst::Neg32 { dst, src, .. } => {
            format!("{} = Neg32 {}", ireg(dst), ireg(src))
        }
        VInst::Bnot32 { dst, src, .. } => {
            format!("{} = Bnot32 {}", ireg(dst), ireg(src))
        }
        VInst::Mov32 { dst, src, .. } => {
            format!("{} = Mov32 {}", ireg(dst), ireg(src))
        }

        VInst::Icmp32 { dst, lhs, rhs, cond, .. } => {
            format!("{} = Icmp32 {}, {}, {}", ireg(dst), icmp_cond(cond), ireg(lhs), ireg(rhs))
        }
        VInst::IeqImm32 { dst, src, imm, .. } => {
            format!("{} = IeqImm32 {}, {}", ireg(dst), ireg(src), imm)
        }
        VInst::Select32 { dst, cond, if_true, if_false, .. } => {
            format!("{} = Select32 {}, {}, {}", ireg(dst), ireg(cond), ireg(if_true), ireg(if_false))
        }

        VInst::Load32 { dst, base, offset, .. } => {
            if *offset == 0 {
                format!("{} = Load32 {}", ireg(dst), ireg(base))
            } else {
                format!("{} = Load32 {}, {}", ireg(dst), ireg(base), offset)
            }
        }
        VInst::Store32 { src, base, offset, .. } => {
            if *offset == 0 {
                format!("Store32 {}, {}", ireg(src), ireg(base))
            } else {
                format!("Store32 {}, {}, {}", ireg(src), ireg(base), offset)
            }
        }
        VInst::SlotAddr { dst, slot, .. } => {
            format!("{} = SlotAddr {}", ireg(dst), slot)
        }
        VInst::MemcpyWords { dst_base, src_base, size, .. } => {
            format!("MemcpyWords {}, {}, {}", ireg(dst_base), ireg(src_base), size)
        }

        VInst::Call { target, args, rets, .. } => {
            let target_str = target.name.as_str();
            let args_str = format_args(args);
            if rets.is_empty() {
                format!("Call {} {}", target_str, args_str)
            } else if rets.len() == 1 {
                format!("{} = Call {} {}", ireg(&rets[0]), target_str, args_str)
            } else {
                let rets_str = format_rets(rets);
                format!("{} = Call {} {}", rets_str, target_str, args_str)
            }
        }

        VInst::Ret { vals, .. } => {
            if vals.is_empty() {
                "Ret".into()
            } else if vals.len() == 1 {
                format!("Ret {}", ireg(&vals[0]))
            } else {
                format!("Ret {}", format_rets(vals))
            }
        }

        VInst::Br { target, .. } => {
            format!("Br @{}", target)
        }
        VInst::BrIf { cond, target, .. } => {
            format!("BrIf {}, @{}", ireg(cond), target)
        }
    }
}

fn ireg(v: &VReg) -> String {
    format!("i{}", v.0)
}

fn icmp_cond(cond: &IcmpCond) -> &'static str {
    match cond {
        IcmpCond::Eq => "Eq",
        IcmpCond::Ne => "Ne",
        IcmpCond::LtS => "LtS",
        IcmpCond::LeS => "LeS",
        IcmpCond::GtS => "GtS",
        IcmpCond::GeS => "GeS",
        IcmpCond::LtU => "LtU",
        IcmpCond::LeU => "LeU",
        IcmpCond::GtU => "GtU",
        IcmpCond::GeU => "GeU",
    }
}

fn format_args(args: &[VReg]) -> String {
    let parts: Vec<_> = args.iter().map(ireg).collect();
    format!("({})", parts.join(", "))
}

fn format_rets(rets: &[VReg]) -> String {
    let parts: Vec<_> = rets.iter().map(ireg).collect();
    format!("({})", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_iconst() {
        let input = "i0 = IConst32 42";
        let vinsts = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        assert!(matches!(&vinsts[0], VInst::IConst32 { dst, val, .. } if dst.0 == 0 && *val == 42));
    }

    #[test]
    fn test_parse_add() {
        let input = "i2 = Add32 i0, i1";
        let vinsts = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        assert!(matches!(&vinsts[0], VInst::Add32 { dst, src1, src2, .. } if dst.0 == 2 && src1.0 == 0 && src2.0 == 1));
    }

    #[test]
    fn test_parse_icmp() {
        let input = "i3 = Icmp32 Eq, i0, i1";
        let vinsts = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        assert!(matches!(&vinsts[0], VInst::Icmp32 { dst, cond, .. } if dst.0 == 3 && *cond == IcmpCond::Eq));
    }

    #[test]
    fn test_parse_call() {
        let input = "(i2, i3) = Call mod (i0, i1)";
        let vinsts = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        match &vinsts[0] {
            VInst::Call { rets, args, .. } => {
                assert_eq!(rets.len(), 2);
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_parse_ret() {
        let input = "Ret i0";
        let vinsts = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        match &vinsts[0] {
            VInst::Ret { vals, .. } => {
                assert_eq!(vals.len(), 1);
                assert_eq!(vals[0].0, 0);
            }
            _ => panic!("Expected Ret"),
        }
    }

    #[test]
    fn test_parse_label() {
        let input = "@0:";
        let vinsts = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        assert!(matches!(vinsts[0], VInst::Label(0, None)));
    }

    #[test]
    fn test_parse_br() {
        let input = "Br @0";
        let vinsts = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        assert!(matches!(vinsts[0], VInst::Br { target: 0, .. }));
    }

    #[test]
    fn test_parse_brif() {
        let input = "BrIf i0, @1";
        let vinsts = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        assert!(matches!(&vinsts[0], VInst::BrIf { cond, target: 1, .. } if cond.0 == 0));
    }

    #[test]
    fn test_format_iconst() {
        let inst = VInst::IConst32 { dst: VReg(0), val: 42, src_op: None };
        let s = format_vinst(&inst);
        assert_eq!(s, "i0 = IConst32 42");
    }

    #[test]
    fn test_format_add() {
        let inst = VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), src_op: None };
        let s = format_vinst(&inst);
        assert_eq!(s, "i2 = Add32 i0, i1");
    }

    #[test]
    fn test_format_icmp() {
        let inst = VInst::Icmp32 { dst: VReg(3), lhs: VReg(0), rhs: VReg(1), cond: IcmpCond::Eq, src_op: None };
        let s = format_vinst(&inst);
        assert_eq!(s, "i3 = Icmp32 Eq, i0, i1");
    }

    #[test]
    fn test_roundtrip() {
        let input = r#"
i0 = IConst32 1
i1 = IConst32 2
i2 = Add32 i0, i1
Ret i2
"#;
        let vinsts = parse(input).unwrap();
        let output = format(&vinsts);
        let reparsed = parse(&output).unwrap();
        assert_eq!(vinsts.len(), reparsed.len());
    }
}
