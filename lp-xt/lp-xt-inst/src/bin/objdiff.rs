//! Differential disassembler conformance rig (host-only).
//!
//! Disassembles the entire `.text` of an ELF with `lp-xt-inst` and diffs against
//! `xtensa-esp32s3-elf-objdump -d`. Every instruction is either MATCHED (its
//! mnemonic and operand values agree, resolving hex/decimal/target formatting
//! differences), or placed on the printed UNSUPPORTED allowlist (counted by
//! mnemonic, never silently skipped). Data directives (`.byte`, `.long`) are
//! counted apart.
//!
//! Usage: `cargo run -p lp-xt-inst --features objdiff --bin objdiff -- <elf> [objdump]`

use lp_xt_inst::disasm::l32r_target;
use lp_xt_inst::{DecodeError, Inst, decode};
use object::{Object, ObjectSection};
use std::collections::BTreeMap;
use std::process::Command;

const DEFAULT_OBJDUMP: &str = "/Users/yona/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/xtensa-esp-elf/bin/xtensa-esp32s3-elf-objdump";

#[derive(Clone, PartialEq, Debug)]
enum Operand {
    /// An address register, rendered `a3`.
    Reg(u8),
    /// A float register, rendered `f3`. A separate kind so an operand-order
    /// mistake between the AR and FR files cannot compare equal.
    FReg(u8),
    /// A boolean register, rendered `b3`.
    BReg(u8),
    Imm(i64),
    Addr(u32),
}

fn main() {
    let mut args = std::env::args().skip(1);
    let elf_path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("usage: objdiff <elf> [objdump-path]");
            std::process::exit(2);
        }
    };
    let objdump = args.next().unwrap_or_else(|| {
        std::env::var("XT_OBJDUMP").unwrap_or_else(|_| DEFAULT_OBJDUMP.to_string())
    });

    // --- load .text via the object crate ---
    let data = std::fs::read(&elf_path).expect("read elf");
    let file = object::File::parse(&*data).expect("parse elf");
    let text = file
        .section_by_name(".text")
        .expect(".text section present");
    let text_vma = text.address() as u32;
    let text_bytes = text.data().expect(".text data").to_vec();
    let text_end = text_vma + text_bytes.len() as u32;

    // --- run objdump -d -j .text ---
    let out = Command::new(&objdump)
        .args(["-d", "-j", ".text", &elf_path])
        .output()
        .expect("spawn objdump");
    if !out.status.success() {
        eprintln!("objdump failed: {}", String::from_utf8_lossy(&out.stderr));
        std::process::exit(1);
    }
    let listing = String::from_utf8_lossy(&out.stdout);

    let mut n_matched = 0usize;
    let mut n_data = 0usize;
    let mut n_insns = 0usize;
    let mut unsupported: BTreeMap<String, usize> = BTreeMap::new();
    let mut supported: BTreeMap<String, usize> = BTreeMap::new();
    let mut mismatches: Vec<String> = Vec::new();

    for line in listing.lines() {
        let Some((addr, hex, text_field)) = parse_line(line) else {
            continue;
        };
        // Only consider instructions that live inside .text bounds we loaded.
        if addr < text_vma || addr >= text_end {
            continue;
        }
        let od_mnem = text_field.split_whitespace().next().unwrap_or("");
        if od_mnem.starts_with('.') || od_mnem.is_empty() {
            // objdump data directive (.byte/.short/.long/.word) — count as data.
            n_data += hex.len() / 2;
            continue;
        }
        n_insns += 1;

        let len = hex.len() / 2; // objdump byte count for this instruction
        let off = (addr - text_vma) as usize;
        if off + len > text_bytes.len() {
            continue;
        }
        let bytes = &text_bytes[off..off + len];

        match decode(bytes) {
            Ok((inst, my_len)) => {
                *supported.entry(od_mnem.to_string()).or_default() += 1;
                let (my_mnem, my_ops_raw) = canonical(&inst, addr);
                let my_ops: Vec<Operand> = my_ops_raw.iter().map(normalize).collect();
                let od_ops = parse_operands(&text_field, &my_ops_raw)
                    .map(|v| v.iter().map(normalize).collect::<Vec<_>>());
                let ok = my_len == len
                    && my_mnem == od_mnem
                    && od_ops.as_deref() == Some(my_ops.as_slice());
                if ok {
                    n_matched += 1;
                } else if mismatches.len() < 50 {
                    mismatches.push(format!(
                        "  {addr:#010x}  bytes={hex}\n      objdump: {}\n      lp-xt:   {} {:?} (len {my_len} vs {len})",
                        text_field.trim(),
                        my_mnem,
                        my_ops,
                    ));
                } else {
                    // still counted as a mismatch via the (n_insns - matched - ...) math
                }
            }
            Err(DecodeError::Unsupported { .. }) => {
                *unsupported.entry(od_mnem.to_string()).or_default() += 1;
            }
            Err(DecodeError::Truncated { .. }) => {
                *unsupported
                    .entry(format!("{od_mnem} (truncated)"))
                    .or_default() += 1;
            }
        }
    }

    let n_unsupported: usize = unsupported.values().sum();
    let n_supported_seen: usize = supported.values().sum();
    let n_mismatched = n_supported_seen - n_matched;

    println!("=== lp-xt-inst objdiff over {elf_path} ===");
    println!(".text: vma={text_vma:#x} size={} bytes", text_bytes.len());
    println!();
    println!("instructions (objdump):   {n_insns}");
    println!("  supported (decoded):    {n_supported_seen}");
    println!("    matched:              {n_matched}");
    println!("    MISMATCHED:           {n_mismatched}");
    println!("  unsupported:            {n_unsupported}");
    println!("data-directive bytes:     {n_data}");
    println!();

    println!("--- supported opcodes ({} kinds) ---", supported.len());
    let mut sup: Vec<_> = supported.iter().collect();
    sup.sort_by(|a, b| b.1.cmp(a.1));
    for (m, c) in sup {
        println!("  {c:>6}  {m}");
    }
    println!();

    println!(
        "--- UNSUPPORTED allowlist ({} kinds) ---",
        unsupported.len()
    );
    let mut uns: Vec<_> = unsupported.iter().collect();
    uns.sort_by(|a, b| b.1.cmp(a.1));
    for (m, c) in uns {
        println!("  {c:>6}  {m}");
    }
    println!();

    if !mismatches.is_empty() {
        println!("--- MISMATCHES (first {}) ---", mismatches.len());
        for m in &mismatches {
            println!("{m}");
        }
        println!();
    }

    if n_mismatched == 0 {
        println!("RESULT: PASS — zero mismatches on the supported subset.");
    } else {
        println!("RESULT: FAIL — {n_mismatched} mismatches on supported ops.");
        std::process::exit(1);
    }
}

/// Parse one objdump listing line into `(address, hex-bytes, insn-text)`.
fn parse_line(line: &str) -> Option<(u32, String, String)> {
    // Format: "   <hexaddr>:\t<hex bytes>  \t<mnemonic>\t<operands>"
    let (addr_part, rest) = line.split_once(':')?;
    let addr = u32::from_str_radix(addr_part.trim(), 16).ok()?;
    let mut fields = rest.split('\t').filter(|f| !f.trim().is_empty());
    let hex_raw = fields.next()?;
    let hex: String = hex_raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.is_empty() {
        return None;
    }
    let text: String = fields.collect::<Vec<_>>().join(" ");
    if text.trim().is_empty() {
        return None;
    }
    Some((addr, hex, text))
}

/// Canonical `(mnemonic, operands)` for a decoded instruction, as the comparison
/// oracle. Mirrors [`lp_xt_inst::format_instruction`] but yields typed operands.
fn canonical(inst: &Inst, pc: u32) -> (String, Vec<Operand>) {
    let s = lp_xt_inst::disasm::format_inst(inst, pc);
    let mnem = match s.split_once('\t') {
        Some((m, _)) => m.to_string(),
        None => s,
    };
    (mnem, typed_operands(inst, pc))
}

/// Typed operand list for an instruction (kinds match positionally with objdump).
fn typed_operands(inst: &Inst, pc: u32) -> Vec<Operand> {
    use lp_xt_inst::Inst::*;
    let reg = |r: lp_xt_inst::Reg| Operand::Reg(r.num());
    let freg = |r: lp_xt_inst::FReg| Operand::FReg(r.num());
    let breg = |r: lp_xt_inst::BReg| Operand::BReg(r.num());
    let br = |off: i32| Operand::Addr(pc.wrapping_add(4).wrapping_add(off as u32));
    match *inst {
        Rrr(_, a, b, c) => vec![reg(a), reg(b), reg(c)],
        Rt(_, a, b) => vec![reg(a), reg(b)],
        Rs(_, a, b) => vec![reg(a), reg(b)],
        ShiftSet(_, a) => vec![reg(a)],
        Ssai(i) => vec![Operand::Imm(i as i64)],
        Slli(a, b, sa) | Srli(a, b, sa) | Srai(a, b, sa) => {
            vec![reg(a), reg(b), Operand::Imm(sa as i64)]
        }
        Extui(a, b, sh, mk) => vec![
            reg(a),
            reg(b),
            Operand::Imm(sh as i64),
            Operand::Imm(mk as i64),
        ],
        Sext(a, b, i) => vec![reg(a), reg(b), Operand::Imm(i as i64)],
        MovN(a, b) => vec![reg(a), reg(b)],
        AddN(a, b, c) => vec![reg(a), reg(b), reg(c)],
        AddiN(a, b, i) => vec![reg(a), reg(b), Operand::Imm(i as i64)],
        Addi(a, b, i) | Addmi(a, b, i) => vec![reg(a), reg(b), Operand::Imm(i as i64)],
        Movi(a, i) => vec![reg(a), Operand::Imm(i as i64)],
        MoviN(a, i) => vec![reg(a), Operand::Imm(i as i64)],
        Load(_, a, b, off) | Store(_, a, b, off) => vec![reg(a), reg(b), Operand::Imm(off as i64)],
        L32iN(a, b, off) | S32iN(a, b, off) => vec![reg(a), reg(b), Operand::Imm(off as i64)],
        L32r(a, imm16) => vec![reg(a), Operand::Addr(l32r_target(pc, imm16))],
        BranchRr(_, a, b, off) => vec![reg(a), reg(b), br(off)],
        BranchRi(_, a, v, off) => vec![reg(a), Operand::Imm(v as i64), br(off)],
        BranchRiu(_, a, v, off) => vec![reg(a), Operand::Imm(v as i64), br(off)],
        BranchZ(_, a, off) => vec![reg(a), br(off)],
        BranchBiI(_, a, imm, off) => vec![reg(a), Operand::Imm(imm as i64), br(off)],
        BranchZN(_, a, imm6) => vec![reg(a), br(imm6 as i32)],
        J(off) => vec![br(off)],
        Jx(a) => vec![reg(a)],
        Call(_, off) => vec![Operand::Addr(
            (pc & !3).wrapping_add((off as u32) << 2).wrapping_add(4),
        )],
        Callx(_, a) => vec![reg(a)],
        Entry(a, imm) => vec![reg(a), Operand::Imm(imm as i64)],
        Nullary(_) | NullaryN(_) => vec![],

        // --- floating point ---
        FpRrr(_, a, b, c) => vec![freg(a), freg(b), freg(c)],
        FpRr(_, a, b) => vec![freg(a), freg(b)],
        ConstS(a, imm) => vec![freg(a), Operand::Imm(imm as i64)],
        Rfr(a, b) => vec![reg(a), freg(b)],
        Wfr(a, b) => vec![freg(a), reg(b)],
        FpMovAr(_, a, b, c) => vec![freg(a), freg(b), reg(c)],
        FpMovBr(_, a, b, c) => vec![freg(a), freg(b), breg(c)],
        FpCmp(_, a, b, c) => vec![breg(a), freg(b), freg(c)],
        FpToInt(_, a, b, imm) => vec![reg(a), freg(b), Operand::Imm(imm as i64)],
        IntToFp(_, a, b, imm) => vec![freg(a), reg(b), Operand::Imm(imm as i64)],
        FpLsx(_, a, b, c) => vec![freg(a), reg(b), reg(c)],
        FpLsi(_, a, b, off) => vec![freg(a), reg(b), Operand::Imm(off as i64)],

        // --- boolean register file ---
        MovBool(_, a, b, c) => vec![reg(a), reg(b), breg(c)],
        BranchBool(_, a, off) => vec![breg(a), br(off)],

        // --- special / user registers: the register is part of the mnemonic ---
        Sr(_, _, a) | Ur(_, _, a) => vec![reg(a)],
    }
}

/// Parse objdump's operand text into typed operands using positional kind hints.
/// Returns `None` if the count or a token fails to parse.
fn parse_operands(text: &str, hints: &[Operand]) -> Option<Vec<Operand>> {
    let rest = text
        .split_once(char::is_whitespace)
        .map(|(_, r)| r)
        .unwrap_or("");
    let rest = rest.trim();
    if rest.is_empty() {
        return if hints.is_empty() { Some(vec![]) } else { None };
    }
    let toks: Vec<&str> = rest.split(',').map(|t| t.trim()).collect();
    if toks.len() != hints.len() {
        return None;
    }
    let mut out = Vec::with_capacity(toks.len());
    for (tok, hint) in toks.iter().zip(hints) {
        // strip trailing "<symbol>" annotations
        let tok = tok.split('<').next().unwrap_or(tok).trim();
        let val = match hint {
            Operand::Reg(_) => {
                let n = tok.strip_prefix('a')?;
                Operand::Reg(n.parse().ok()?)
            }
            Operand::FReg(_) => {
                let n = tok.strip_prefix('f')?;
                Operand::FReg(n.parse().ok()?)
            }
            Operand::BReg(_) => {
                let n = tok.strip_prefix('b')?;
                Operand::BReg(n.parse().ok()?)
            }
            Operand::Imm(_) => Operand::Imm(parse_int(tok)? & 0xffff_ffff),
            Operand::Addr(_) => {
                Operand::Addr(u32::from_str_radix(tok.trim_start_matches("0x"), 16).ok()?)
            }
        };
        out.push(val);
    }
    Some(out)
}

/// Parse a signed integer that may be hex (`0x..`, `-0x..`) or decimal.
fn parse_int(s: &str) -> Option<i64> {
    let (neg, body) = match s.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, s),
    };
    let v = if let Some(h) = body.strip_prefix("0x") {
        i64::from_str_radix(h, 16).ok()?
    } else {
        body.parse::<i64>().ok()?
    };
    Some(if neg { -v } else { v })
}

/// Normalize an operand so signed/hex renderings of the same value compare equal
/// (immediates masked to 32 bits).
fn normalize(op: &Operand) -> Operand {
    match op {
        Operand::Imm(v) => Operand::Imm(v & 0xffff_ffff),
        other => other.clone(),
    }
}
