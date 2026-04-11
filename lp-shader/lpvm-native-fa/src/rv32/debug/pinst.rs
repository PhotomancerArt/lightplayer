//! [`PInst`](crate::rv32::inst::PInst) parser and formatter (RISC-V assembly style).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::rv32::gpr::{self, PReg};
use crate::rv32::inst::PInst;
use crate::rv32::inst::SymbolRef;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

impl ParseError {
    pub fn new(line: usize, msg: impl Into<String>) -> Self {
        Self {
            line,
            msg: msg.into(),
        }
    }
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

fn reg(r: PReg) -> &'static str {
    gpr::reg_name(r)
}

pub fn format(inst: &PInst) -> String {
    match inst {
        PInst::FrameSetup { spill_slots } => format!("FrameSetup {}", spill_slots),
        PInst::FrameTeardown { spill_slots } => format!("FrameTeardown {}", spill_slots),

        PInst::Add { dst, src1, src2 } => {
            format!("add {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }
        PInst::Sub { dst, src1, src2 } => {
            format!("sub {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }
        PInst::Mul { dst, src1, src2 } => {
            format!("mul {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }
        PInst::Div { dst, src1, src2 } => {
            format!("div {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }
        PInst::Divu { dst, src1, src2 } => {
            format!("divu {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }
        PInst::Rem { dst, src1, src2 } => {
            format!("rem {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }
        PInst::Remu { dst, src1, src2 } => {
            format!("remu {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }

        PInst::And { dst, src1, src2 } => {
            format!("and {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }
        PInst::Or { dst, src1, src2 } => {
            format!("or {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }
        PInst::Xor { dst, src1, src2 } => {
            format!("xor {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }

        PInst::Sll { dst, src1, src2 } => {
            format!("sll {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }
        PInst::Srl { dst, src1, src2 } => {
            format!("srl {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }
        PInst::Sra { dst, src1, src2 } => {
            format!("sra {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }

        PInst::Neg { dst, src } => format!("neg {}, {}", reg(*dst), reg(*src)),
        PInst::Not { dst, src } => format!("not {}, {}", reg(*dst), reg(*src)),
        PInst::Mv { dst, src } => format!("mv {}, {}", reg(*dst), reg(*src)),

        PInst::Slt { dst, src1, src2 } => {
            format!("slt {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }
        PInst::Sltu { dst, src1, src2 } => {
            format!("sltu {}, {}, {}", reg(*dst), reg(*src1), reg(*src2))
        }
        PInst::Seqz { dst, src } => format!("seqz {}, {}", reg(*dst), reg(*src)),
        PInst::Snez { dst, src } => format!("snez {}, {}", reg(*dst), reg(*src)),
        PInst::Sltz { dst, src } => format!("sltz {}, {}", reg(*dst), reg(*src)),
        PInst::Sgtz { dst, src } => format!("sgtz {}, {}", reg(*dst), reg(*src)),

        PInst::Li { dst, imm } => format!("li {}, {}", reg(*dst), imm),
        PInst::Addi { dst, src, imm } => format!("addi {}, {}, {}", reg(*dst), reg(*src), imm),

        PInst::Lw { dst, base, offset } => {
            format!("lw {}, {}({})", reg(*dst), offset, reg(*base))
        }
        PInst::Sw { src, base, offset } => {
            format!("sw {}, {}({})", reg(*src), offset, reg(*base))
        }

        PInst::SlotAddr { dst, slot } => format!("SlotAddr {}, {}", reg(*dst), slot),

        PInst::MemcpyWords { dst, src, size } => {
            format!("MemcpyWords {}, {}, {}", reg(*dst), reg(*src), size)
        }

        PInst::Call { target } => format!("call {}", target.name),
        PInst::Ret => String::from("ret"),
        PInst::Beq { src1, src2, target } => {
            format!("beq {}, {}, @{}", reg(*src1), reg(*src2), target)
        }
        PInst::Bne { src1, src2, target } => {
            format!("bne {}, {}, @{}", reg(*src1), reg(*src2), target)
        }
        PInst::Blt { src1, src2, target } => {
            format!("blt {}, {}, @{}", reg(*src1), reg(*src2), target)
        }
        PInst::Bge { src1, src2, target } => {
            format!("bge {}, {}, @{}", reg(*src1), reg(*src2), target)
        }
        PInst::J { target } => format!("j @{}", target),
        PInst::Label { id } => format!("label @{}:", id),
    }
}

pub fn format_block(inst: &[PInst]) -> String {
    inst.iter().map(format).collect::<Vec<_>>().join("\n")
}

fn trim_comment(s: &str) -> &str {
    s.split('#').next().unwrap_or(s).trim()
}

fn parse_i32(s: &str) -> Result<i32, ()> {
    s.trim().parse::<i32>().map_err(|_| ())
}

fn parse_u32(s: &str) -> Result<u32, ()> {
    s.trim().parse::<u32>().map_err(|_| ())
}

fn parse_mem_operand(s: &str) -> Result<(i32, PReg), ()> {
    let s = s.trim();
    let open = s.find('(').ok_or(())?;
    let close = s.find(')').ok_or(())?;
    if close <= open + 1 {
        return Err(());
    }
    let off = parse_i32(&s[..open])?;
    let base = gpr::parse_reg(s[open + 1..close].trim())?;
    Ok((off, base))
}

fn split_operands(rest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for ch in rest.chars() {
        match ch {
            '(' => {
                depth += 1;
                cur.push(ch);
            }
            ')' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

pub fn parse_line(line: &str, line_no: usize) -> Result<PInst, ParseError> {
    let s = trim_comment(line);
    if s.is_empty() {
        return Err(ParseError::new(line_no, "empty line"));
    }
    let mut parts = s.split_whitespace();
    let mn = parts
        .next()
        .ok_or_else(|| ParseError::new(line_no, "missing mnemonic"))?;
    let rest = s[mn.len()..].trim();
    let ops = split_operands(rest);

    let r = |i: usize| -> Result<PReg, ParseError> {
        ops.get(i)
            .ok_or_else(|| ParseError::new(line_no, "missing operand"))
            .and_then(|s| gpr::parse_reg(s).map_err(|_| ParseError::new(line_no, "bad register")))
    };

    match mn {
        "FrameSetup" => Ok(PInst::FrameSetup {
            spill_slots: parse_u32(ops.get(0).map(|s| s.as_str()).unwrap_or("0"))
                .map_err(|_| ParseError::new(line_no, "bad spill_slots"))?,
        }),
        "FrameTeardown" => Ok(PInst::FrameTeardown {
            spill_slots: parse_u32(ops.get(0).map(|s| s.as_str()).unwrap_or("0"))
                .map_err(|_| ParseError::new(line_no, "bad spill_slots"))?,
        }),
        "add" => Ok(PInst::Add {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "sub" => Ok(PInst::Sub {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "mul" => Ok(PInst::Mul {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "div" => Ok(PInst::Div {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "divu" => Ok(PInst::Divu {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "rem" => Ok(PInst::Rem {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "remu" => Ok(PInst::Remu {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "and" => Ok(PInst::And {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "or" => Ok(PInst::Or {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "xor" => Ok(PInst::Xor {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "sll" => Ok(PInst::Sll {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "srl" => Ok(PInst::Srl {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "sra" => Ok(PInst::Sra {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "neg" => Ok(PInst::Neg {
            dst: r(0)?,
            src: r(1)?,
        }),
        "not" => Ok(PInst::Not {
            dst: r(0)?,
            src: r(1)?,
        }),
        "mv" => Ok(PInst::Mv {
            dst: r(0)?,
            src: r(1)?,
        }),
        "slt" => Ok(PInst::Slt {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "sltu" => Ok(PInst::Sltu {
            dst: r(0)?,
            src1: r(1)?,
            src2: r(2)?,
        }),
        "seqz" => Ok(PInst::Seqz {
            dst: r(0)?,
            src: r(1)?,
        }),
        "snez" => Ok(PInst::Snez {
            dst: r(0)?,
            src: r(1)?,
        }),
        "sltz" => Ok(PInst::Sltz {
            dst: r(0)?,
            src: r(1)?,
        }),
        "sgtz" => Ok(PInst::Sgtz {
            dst: r(0)?,
            src: r(1)?,
        }),
        "li" => Ok(PInst::Li {
            dst: r(0)?,
            imm: parse_i32(ops.get(1).map(|s| s.as_str()).unwrap_or("0"))
                .map_err(|_| ParseError::new(line_no, "bad imm"))?,
        }),
        "addi" => Ok(PInst::Addi {
            dst: r(0)?,
            src: r(1)?,
            imm: parse_i32(ops.get(2).map(|s| s.as_str()).unwrap_or("0"))
                .map_err(|_| ParseError::new(line_no, "bad imm"))?,
        }),
        "lw" => {
            let dst = r(0)?;
            let (off, base) = parse_mem_operand(
                ops.get(1)
                    .ok_or_else(|| ParseError::new(line_no, "missing mem"))?,
            )
            .map_err(|_| ParseError::new(line_no, "bad lw operand"))?;
            Ok(PInst::Lw {
                dst,
                base,
                offset: off,
            })
        }
        "sw" => {
            let src = r(0)?;
            let (off, base) = parse_mem_operand(
                ops.get(1)
                    .ok_or_else(|| ParseError::new(line_no, "missing mem"))?,
            )
            .map_err(|_| ParseError::new(line_no, "bad sw operand"))?;
            Ok(PInst::Sw {
                src,
                base,
                offset: off,
            })
        }
        "SlotAddr" => Ok(PInst::SlotAddr {
            dst: r(0)?,
            slot: parse_u32(ops.get(1).map(|s| s.as_str()).unwrap_or("0"))
                .map_err(|_| ParseError::new(line_no, "bad slot"))?,
        }),
        "MemcpyWords" => Ok(PInst::MemcpyWords {
            dst: r(0)?,
            src: r(1)?,
            size: parse_u32(ops.get(2).map(|s| s.as_str()).unwrap_or("0"))
                .map_err(|_| ParseError::new(line_no, "bad size"))?,
        }),
        "call" => {
            let name = ops
                .get(0)
                .ok_or_else(|| ParseError::new(line_no, "missing symbol"))?
                .clone();
            Ok(PInst::Call {
                target: SymbolRef { name },
            })
        }
        "ret" => {
            if !rest.is_empty() {
                return Err(ParseError::new(line_no, "unexpected operands on ret"));
            }
            Ok(PInst::Ret)
        }
        "beq" | "bne" | "blt" | "bge" => {
            let a = r(0)?;
            let b = r(1)?;
            let t = ops
                .get(2)
                .ok_or_else(|| ParseError::new(line_no, "missing target"))?;
            let t = t.trim();
            let id = if let Some(rest) = t.strip_prefix('@') {
                parse_u32(rest).map_err(|_| ParseError::new(line_no, "bad label"))?
            } else {
                return Err(ParseError::new(line_no, "branch target must be @N"));
            };
            Ok(match mn {
                "beq" => PInst::Beq {
                    src1: a,
                    src2: b,
                    target: id,
                },
                "bne" => PInst::Bne {
                    src1: a,
                    src2: b,
                    target: id,
                },
                "blt" => PInst::Blt {
                    src1: a,
                    src2: b,
                    target: id,
                },
                "bge" => PInst::Bge {
                    src1: a,
                    src2: b,
                    target: id,
                },
                _ => unreachable!(),
            })
        }
        "j" => {
            let t = ops
                .get(0)
                .ok_or_else(|| ParseError::new(line_no, "missing target"))?;
            let t = t.trim();
            let id = if let Some(rest) = t.strip_prefix('@') {
                parse_u32(rest).map_err(|_| ParseError::new(line_no, "bad label"))?
            } else {
                return Err(ParseError::new(line_no, "jump target must be @N"));
            };
            Ok(PInst::J { target: id })
        }
        _ => Err(ParseError::new(line_no, format!("unknown mnemonic `{mn}`"))),
    }
}

pub fn parse(input: &str) -> Result<Vec<PInst>, ParseError> {
    let mut out = Vec::new();
    for (i, line) in input.lines().enumerate() {
        let line_no = i + 1;
        let t = trim_comment(line);
        if t.is_empty() {
            continue;
        }
        out.push(parse_line(line, line_no)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_add() {
        let inst = PInst::Add {
            dst: 10,
            src1: 11,
            src2: 12,
        };
        assert_eq!(format(&inst), "add a0, a1, a2");
    }

    #[test]
    fn test_format_li() {
        let inst = PInst::Li { dst: 10, imm: 42 };
        assert_eq!(format(&inst), "li a0, 42");
    }

    #[test]
    fn test_format_lw() {
        let inst = PInst::Lw {
            dst: 10,
            base: 2,
            offset: 4,
        };
        assert_eq!(format(&inst), "lw a0, 4(sp)");
    }

    #[test]
    fn test_format_sw() {
        let inst = PInst::Sw {
            src: 10,
            base: 2,
            offset: 8,
        };
        assert_eq!(format(&inst), "sw a0, 8(sp)");
    }

    #[test]
    fn test_format_ret() {
        let inst = PInst::Ret;
        assert_eq!(format(&inst), "ret");
    }

    #[test]
    fn test_parse_add() {
        let inst = parse_line("add a0, a1, a2", 1).unwrap();
        assert!(matches!(
            inst,
            PInst::Add {
                dst: 10,
                src1: 11,
                src2: 12
            }
        ));
    }

    #[test]
    fn test_parse_li() {
        let inst = parse_line("li a0, 42", 1).unwrap();
        assert!(matches!(inst, PInst::Li { dst: 10, imm: 42 }));
    }

    #[test]
    fn test_parse_ret() {
        let inst = parse_line("ret", 1).unwrap();
        assert!(matches!(inst, PInst::Ret));
    }

    #[test]
    fn test_roundtrip_add() {
        let original = PInst::Add {
            dst: 10,
            src1: 11,
            src2: 12,
        };
        let text = format(&original);
        let parsed = parse_line(&text, 1).unwrap();
        assert_eq!(original, parsed);
    }
}
