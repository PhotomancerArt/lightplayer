//! VInst text format and parser.
//!
//! Format:
//!   i2 = Add i0, i1            // binary ALU op (AluRRR)
//!   i2 = Addi i0, 42           // immediate ALU op (AluRRI)
//!   i0 = IConst32 42           // immediate
//!   i3 = Icmp Eq, i0, i1      // comparison (cond first)
//!   i3 = IcmpImm Eq, i0, 42   // immediate comparison
//!   i3 = Select i0, i1, i2    // cond, if_true, if_false
//!   i1 = Load32 i0, 4          // base, offset (optional)
//!   Store32 i1, i0, 4          // src, base, offset
//!   (i2, i3) = Call mod (i0, i1)  // multi-ret and args
//!   Ret i0                     // return values
//!   Br @0                      // branch to label
//!   BrIf i0, @1                // branch if i0 != 0
//!   @0:                        // label definition

use crate::vinst::{AluImmOp, AluOp, IcmpCond, ModuleSymbols, SRC_OP_NONE, VInst, VReg, VRegSlice};
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

fn push_vregs(pool: &mut Vec<VReg>, regs: &[VReg]) -> Result<VRegSlice, ParseError> {
    if regs.len() > u8::MAX as usize {
        return Err(ParseError {
            line: 0,
            message: String::from("too many vregs in slice"),
        });
    }
    let start = u16::try_from(pool.len()).map_err(|_| ParseError {
        line: 0,
        message: String::from("vreg pool exhausted"),
    })?;
    pool.extend_from_slice(regs);
    Ok(VRegSlice {
        start,
        count: regs.len() as u8,
    })
}

/// Parse VInsts from text representation.
pub fn parse(input: &str) -> Result<(Vec<VInst>, ModuleSymbols, Vec<VReg>), ParseError> {
    let mut vinsts = Vec::new();
    let mut symbols = ModuleSymbols::default();
    let mut pool = Vec::new();
    for (line_num, line) in input.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let inst = parse_line(line, line_num, &mut symbols, &mut pool)?;
        vinsts.push(inst);
    }
    Ok((vinsts, symbols, pool))
}

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

fn parse_line(
    line: &str,
    line_num: usize,
    symbols: &mut ModuleSymbols,
    pool: &mut Vec<VReg>,
) -> Result<VInst, ParseError> {
    // Label definition: @0:
    if line.starts_with("@") && line.ends_with(":") {
        let id_str = &line[1..line.len() - 1];
        let id: u32 = id_str.parse().map_err(|_| ParseError {
            line: line_num,
            message: format!("Invalid label id: {id_str}"),
        })?;
        return Ok(VInst::Label(id, SRC_OP_NONE));
    }

    // Assignment: i2 = Add i0, i1
    if let Some((lhs, rhs)) = line.split_once(" = ") {
        let dsts = parse_rets(lhs.trim())?;
        return parse_def_instruction(dsts, rhs.trim(), line_num, symbols, pool);
    }

    // No assignment: Store32, Br, BrIf, Ret, Call without ret
    parse_nodef_instruction(line, line_num, symbols, pool)
}

/// Parse return register(s), e.g., "i2" or "(i2, i3)"
fn parse_rets(s: &str) -> Result<Vec<VReg>, ParseError> {
    if s.starts_with("(") && s.ends_with(")") {
        // Multi-ret: (i2, i3)
        let inner = &s[1..s.len() - 1];
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
            message: format!("Expected ireg like 'i0', got '{s}'"),
        });
    }
    let num: u32 = s[1..].parse().map_err(|_| ParseError {
        line: 0,
        message: format!("Invalid ireg number in '{s}'"),
    })?;
    Ok(VReg(num as u16))
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
            message: format!("Expected parens like '(i0, i1)', got '{s}'"),
        });
    }
    let inner = &s[1..s.len() - 1];
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
            message: format!("Expected label like '@0', got '{s}'"),
        });
    }
    s[1..].parse().map_err(|_| ParseError {
        line: 0,
        message: format!("Invalid label id in '{s}'"),
    })
}

fn parse_def_instruction(
    dsts: Vec<VReg>,
    rhs: &str,
    line_num: usize,
    symbols: &mut ModuleSymbols,
    pool: &mut Vec<VReg>,
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
                return Err(ParseError {
                    line: line_num,
                    message: "IConst32 needs 1 dst".into(),
                });
            }
            let val: i32 = args_str.trim().parse().map_err(|_| ParseError {
                line: line_num,
                message: format!("Invalid i32: {args_str}"),
            })?;
            Ok(VInst::IConst32 {
                dst: dsts[0],
                val,
                src_op: SRC_OP_NONE,
            })
        }

        _ if AluOp::from_mnemonic(op).is_some() => {
            let alu_op = AluOp::from_mnemonic(op).unwrap();
            if dsts.len() != 1 {
                return Err(ParseError {
                    line: line_num,
                    message: format!("{op} needs 1 dst"),
                });
            }
            let args = parse_args(&args_str)?;
            if args.len() != 2 {
                return Err(ParseError {
                    line: line_num,
                    message: format!("{op} needs 2 args"),
                });
            }
            let (src1, src2) = (args[0], args[1]);
            Ok(VInst::AluRRR {
                op: alu_op,
                dst: dsts[0],
                src1,
                src2,
                src_op: SRC_OP_NONE,
            })
        }

        _ if AluImmOp::from_mnemonic(op).is_some() => {
            let alu_imm_op = AluImmOp::from_mnemonic(op).unwrap();
            if dsts.len() != 1 {
                return Err(ParseError {
                    line: line_num,
                    message: format!("{op} needs 1 dst"),
                });
            }
            let parts: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
            if parts.len() != 2 {
                return Err(ParseError {
                    line: line_num,
                    message: format!("{op} needs 'ireg, imm'"),
                });
            }
            let src = parse_ireg(parts[0])?;
            let imm: i32 = parts[1].parse().map_err(|_| ParseError {
                line: line_num,
                message: format!("Invalid imm: {}", parts[1]),
            })?;
            Ok(VInst::AluRRI {
                op: alu_imm_op,
                dst: dsts[0],
                src,
                imm,
                src_op: SRC_OP_NONE,
            })
        }

        "Neg" | "Bnot" | "Mov" => {
            if dsts.len() != 1 {
                return Err(ParseError {
                    line: line_num,
                    message: format!("{op} needs 1 dst"),
                });
            }
            let src = parse_ireg(args_str.trim())?;
            match op {
                "Neg" => Ok(VInst::Neg {
                    dst: dsts[0],
                    src,
                    src_op: SRC_OP_NONE,
                }),
                "Bnot" => Ok(VInst::Bnot {
                    dst: dsts[0],
                    src,
                    src_op: SRC_OP_NONE,
                }),
                "Mov" => Ok(VInst::Mov {
                    dst: dsts[0],
                    src,
                    src_op: SRC_OP_NONE,
                }),
                _ => unreachable!(),
            }
        }

        "Icmp" => {
            if dsts.len() != 1 {
                return Err(ParseError {
                    line: line_num,
                    message: "Icmp needs 1 dst".into(),
                });
            }
            // Format: Eq, i0, i1
            let parts: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
            if parts.len() != 3 {
                return Err(ParseError {
                    line: line_num,
                    message: "Icmp needs 'Eq, i0, i1'".into(),
                });
            }
            let cond = parse_icmp_cond(parts[0])?;
            let lhs = parse_ireg(parts[1])?;
            let rhs = parse_ireg(parts[2])?;
            Ok(VInst::Icmp {
                dst: dsts[0],
                lhs,
                rhs,
                cond,
                src_op: SRC_OP_NONE,
            })
        }

        "IcmpImm" => {
            if dsts.len() != 1 {
                return Err(ParseError {
                    line: line_num,
                    message: "IcmpImm needs 1 dst".into(),
                });
            }
            // Format: Eq, i0, 42
            let parts: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
            if parts.len() != 3 {
                return Err(ParseError {
                    line: line_num,
                    message: "IcmpImm needs 'Eq, i0, 42'".into(),
                });
            }
            let cond = parse_icmp_cond(parts[0])?;
            let src = parse_ireg(parts[1])?;
            let imm: i32 = parts[2].parse().map_err(|_| ParseError {
                line: line_num,
                message: format!("Invalid imm: {}", parts[2]),
            })?;
            Ok(VInst::IcmpImm {
                dst: dsts[0],
                src,
                imm,
                cond,
                src_op: SRC_OP_NONE,
            })
        }

        "Select" => {
            if dsts.len() != 1 {
                return Err(ParseError {
                    line: line_num,
                    message: "Select needs 1 dst".into(),
                });
            }
            let args = parse_args(&args_str)?;
            if args.len() != 3 {
                return Err(ParseError {
                    line: line_num,
                    message: "Select needs 3 args".into(),
                });
            }
            Ok(VInst::Select {
                dst: dsts[0],
                cond: args[0],
                if_true: args[1],
                if_false: args[2],
                src_op: SRC_OP_NONE,
            })
        }

        "Load32" => {
            if dsts.len() != 1 {
                return Err(ParseError {
                    line: line_num,
                    message: "Load32 needs 1 dst".into(),
                });
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
            Ok(VInst::Load32 {
                dst: dsts[0],
                base,
                offset,
                src_op: SRC_OP_NONE,
            })
        }

        "SlotAddr" => {
            if dsts.len() != 1 {
                return Err(ParseError {
                    line: line_num,
                    message: "SlotAddr needs 1 dst".into(),
                });
            }
            let slot: u32 = args_str.trim().parse().map_err(|_| ParseError {
                line: line_num,
                message: format!("Invalid slot: {args_str}"),
            })?;
            Ok(VInst::SlotAddr {
                dst: dsts[0],
                slot,
                src_op: SRC_OP_NONE,
            })
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
            let target = symbols.intern(target_name);
            let args_slice = push_vregs(pool, &args)?;
            let rets_slice = push_vregs(pool, &dsts)?;
            Ok(VInst::Call {
                target,
                args: args_slice,
                rets: rets_slice,
                callee_uses_sret: false, // TODO: detect from rets.len()
                src_op: SRC_OP_NONE,
            })
        }

        _ => Err(ParseError {
            line: line_num,
            message: format!("Unknown instruction: {op}"),
        }),
    }
}

fn parse_nodef_instruction(
    line: &str,
    line_num: usize,
    symbols: &mut ModuleSymbols,
    pool: &mut Vec<VReg>,
) -> Result<VInst, ParseError> {
    // Ret i0 or Ret (i0, i1) or Ret
    if line == "Ret" || line.starts_with("Ret ") {
        let rest = line
            .strip_prefix("Ret ")
            .or_else(|| line.strip_prefix("Ret"))
            .unwrap_or("");
        let vals = if rest.trim().is_empty() {
            vec![]
        } else if rest.starts_with("(") && rest.ends_with(")") {
            parse_paren_args(rest)?
        } else {
            vec![parse_ireg(rest.trim())?]
        };
        let vals_slice = push_vregs(pool, &vals)?;
        return Ok(VInst::Ret {
            vals: vals_slice,
            src_op: SRC_OP_NONE,
        });
    }

    // Store32 i1, i0, 4
    if let Some(rest) = line.strip_prefix("Store32 ") {
        let parts: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
        if parts.len() < 2 {
            return Err(ParseError {
                line: line_num,
                message: "Store32 needs src, base[, offset]".into(),
            });
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
        return Ok(VInst::Store32 {
            src,
            base,
            offset,
            src_op: SRC_OP_NONE,
        });
    }

    // MemcpyWords i0, i1, 16
    if let Some(rest) = line.strip_prefix("MemcpyWords ") {
        let parts: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
        if parts.len() != 3 {
            return Err(ParseError {
                line: line_num,
                message: "MemcpyWords needs dst_base, src_base, size".into(),
            });
        }
        let dst_base = parse_ireg(parts[0])?;
        let src_base = parse_ireg(parts[1])?;
        let size: u32 = parts[2].parse().map_err(|_| ParseError {
            line: line_num,
            message: format!("Invalid size: {}", parts[2]),
        })?;
        return Ok(VInst::MemcpyWords {
            dst_base,
            src_base,
            size,
            src_op: SRC_OP_NONE,
        });
    }

    // Br @0
    if let Some(rest) = line.strip_prefix("Br ") {
        let target = parse_label(rest.trim())?;
        return Ok(VInst::Br {
            target,
            src_op: SRC_OP_NONE,
        });
    }

    // BrIf i0, @1
    if let Some(rest) = line.strip_prefix("BrIf ") {
        let parts: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
        if parts.len() != 2 {
            return Err(ParseError {
                line: line_num,
                message: "BrIf needs cond, @label".into(),
            });
        }
        let cond = parse_ireg(parts[0])?;
        let target = parse_label(parts[1])?;
        return Ok(VInst::BrIf {
            cond,
            target,
            invert: false,
            src_op: SRC_OP_NONE,
        });
    }

    // Call mod (i0, i1) with no return
    if let Some(rest) = line.strip_prefix("Call ") {
        let open_paren = rest.find('(').unwrap_or(rest.len());
        let target_name = rest[..open_paren].trim();
        let args = if open_paren < rest.len() {
            parse_paren_args(&rest[open_paren..])?
        } else {
            vec![]
        };
        let target = symbols.intern(target_name);
        let args_slice = push_vregs(pool, &args)?;
        let rets_slice = push_vregs(pool, &[])?;
        return Ok(VInst::Call {
            target,
            args: args_slice,
            rets: rets_slice,
            callee_uses_sret: false,
            src_op: SRC_OP_NONE,
        });
    }

    Err(ParseError {
        line: line_num,
        message: format!("Unknown instruction: {line}"),
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
        _ => Err(ParseError {
            line: 0,
            message: format!("Unknown cond: {s}"),
        }),
    }
}

/// Format VInsts for human-readable output.
pub fn format(vinsts: &[VInst], pool: &[VReg], symbols: &ModuleSymbols) -> String {
    vinsts
        .iter()
        .map(|i| format_vinst(i, pool, symbols))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_vinst(inst: &VInst, pool: &[VReg], symbols: &ModuleSymbols) -> String {
    match inst {
        VInst::Label(id, _) => format!("@{id}:"),

        VInst::IConst32 { dst, val, .. } => {
            format!("{} = IConst32 {}", ireg(dst), val)
        }

        VInst::AluRRR { op, dst, src1, src2, .. } => {
            format!("{} = {} {}, {}", ireg(dst), op.mnemonic(), ireg(src1), ireg(src2))
        }
        VInst::AluRRI { op, dst, src, imm, .. } => {
            format!("{} = {} {}, {}", ireg(dst), op.mnemonic(), ireg(src), imm)
        }

        VInst::Neg { dst, src, .. } => {
            format!("{} = Neg {}", ireg(dst), ireg(src))
        }
        VInst::Bnot { dst, src, .. } => {
            format!("{} = Bnot {}", ireg(dst), ireg(src))
        }
        VInst::Mov { dst, src, .. } => {
            format!("{} = Mov {}", ireg(dst), ireg(src))
        }

        VInst::Icmp {
            dst,
            lhs,
            rhs,
            cond,
            ..
        } => {
            format!(
                "{} = Icmp {}, {}, {}",
                ireg(dst),
                icmp_cond(cond),
                ireg(lhs),
                ireg(rhs)
            )
        }
        VInst::IcmpImm { dst, src, imm, cond, .. } => {
            format!("{} = IcmpImm {}, {}, {}", ireg(dst), icmp_cond(cond), ireg(src), imm)
        }
        VInst::Select {
            dst,
            cond,
            if_true,
            if_false,
            ..
        } => {
            format!(
                "{} = Select {}, {}, {}",
                ireg(dst),
                ireg(cond),
                ireg(if_true),
                ireg(if_false)
            )
        }

        VInst::Load32 {
            dst, base, offset, ..
        } => {
            if *offset == 0 {
                format!("{} = Load32 {}", ireg(dst), ireg(base))
            } else {
                format!("{} = Load32 {}, {}", ireg(dst), ireg(base), offset)
            }
        }
        VInst::Store32 {
            src, base, offset, ..
        } => {
            if *offset == 0 {
                format!("Store32 {}, {}", ireg(src), ireg(base))
            } else {
                format!("Store32 {}, {}, {}", ireg(src), ireg(base), offset)
            }
        }
        VInst::SlotAddr { dst, slot, .. } => {
            format!("{} = SlotAddr {}", ireg(dst), slot)
        }
        VInst::MemcpyWords {
            dst_base,
            src_base,
            size,
            ..
        } => {
            format!(
                "MemcpyWords {}, {}, {}",
                ireg(dst_base),
                ireg(src_base),
                size
            )
        }

        VInst::Call {
            target, args, rets, ..
        } => {
            let target_str = symbols.name(*target);
            let args_str = format_args(args.vregs(pool));
            let rets_v = rets.vregs(pool);
            if rets_v.is_empty() {
                format!("Call {target_str} {args_str}")
            } else if rets_v.len() == 1 {
                format!("{} = Call {} {}", ireg(&rets_v[0]), target_str, args_str)
            } else {
                let rets_str = format_rets(rets_v);
                format!("{rets_str} = Call {target_str} {args_str}")
            }
        }

        VInst::Ret { vals, .. } => {
            let vals = vals.vregs(pool);
            if vals.is_empty() {
                "Ret".into()
            } else if vals.len() == 1 {
                format!("Ret {}", ireg(&vals[0]))
            } else {
                format!("Ret {}", format_rets(vals))
            }
        }

        VInst::Br { target, .. } => {
            format!("Br @{target}")
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
        let (vinsts, _, _) = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        assert!(matches!(&vinsts[0], VInst::IConst32 { dst, val, .. } if dst.0 == 0 && *val == 42));
    }

    #[test]
    fn test_parse_add() {
        let input = "i2 = Add i0, i1";
        let (vinsts, _, _) = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        assert!(
            matches!(&vinsts[0], VInst::AluRRR { op: AluOp::Add, dst, src1, src2, .. } if dst.0 == 2 && src1.0 == 0 && src2.0 == 1)
        );
    }

    #[test]
    fn test_parse_icmp() {
        let input = "i3 = Icmp Eq, i0, i1";
        let (vinsts, _, _) = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        assert!(
            matches!(&vinsts[0], VInst::Icmp { dst, cond, .. } if dst.0 == 3 && *cond == IcmpCond::Eq)
        );
    }

    #[test]
    fn test_parse_call() {
        let input = "(i2, i3) = Call mod (i0, i1)";
        let (vinsts, _, pool) = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        match &vinsts[0] {
            VInst::Call { rets, args, .. } => {
                assert_eq!(rets.vregs(&pool).len(), 2);
                assert_eq!(args.vregs(&pool).len(), 2);
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_parse_ret() {
        let input = "Ret i0";
        let (vinsts, _, pool) = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        match &vinsts[0] {
            VInst::Ret { vals, .. } => {
                let vr = vals.vregs(&pool);
                assert_eq!(vr.len(), 1);
                assert_eq!(vr[0].0, 0);
            }
            _ => panic!("Expected Ret"),
        }
    }

    #[test]
    fn test_parse_label() {
        let input = "@0:";
        let (vinsts, _, _) = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        assert!(matches!(vinsts[0], VInst::Label(0, SRC_OP_NONE)));
    }

    #[test]
    fn test_parse_br() {
        let input = "Br @0";
        let (vinsts, _, _) = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        assert!(matches!(vinsts[0], VInst::Br { target: 0, .. }));
    }

    #[test]
    fn test_parse_brif() {
        let input = "BrIf i0, @1";
        let (vinsts, _, _) = parse(input).unwrap();
        assert_eq!(vinsts.len(), 1);
        assert!(matches!(&vinsts[0], VInst::BrIf { cond, target: 1, .. } if cond.0 == 0));
    }

    #[test]
    fn test_format_iconst() {
        let inst = VInst::IConst32 {
            dst: VReg(0),
            val: 42,
            src_op: SRC_OP_NONE,
        };
        let s = format_vinst(&inst, &[], &ModuleSymbols::default());
        assert_eq!(s, "i0 = IConst32 42");
    }

    #[test]
    fn test_format_add() {
        let inst = VInst::AluRRR {
            op: AluOp::Add,
            dst: VReg(2),
            src1: VReg(0),
            src2: VReg(1),
            src_op: SRC_OP_NONE,
        };
        let s = format_vinst(&inst, &[], &ModuleSymbols::default());
        assert_eq!(s, "i2 = Add i0, i1");
    }

    #[test]
    fn test_format_icmp() {
        let inst = VInst::Icmp {
            dst: VReg(3),
            lhs: VReg(0),
            rhs: VReg(1),
            cond: IcmpCond::Eq,
            src_op: SRC_OP_NONE,
        };
        let s = format_vinst(&inst, &[], &ModuleSymbols::default());
        assert_eq!(s, "i3 = Icmp Eq, i0, i1");
    }

    #[test]
    fn test_roundtrip() {
        let input = r#"
i0 = IConst32 1
i1 = IConst32 2
i2 = Add i0, i1
Ret i2
"#;
        let (vinsts, syms, pool) = parse(input).unwrap();
        let output = format(&vinsts, &pool, &syms);
        let (reparsed, _, _) = parse(&output).unwrap();
        assert_eq!(vinsts.len(), reparsed.len());
    }
}
