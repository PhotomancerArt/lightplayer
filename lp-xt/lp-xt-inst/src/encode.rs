// Encoding data derived from espressif/llvm-project
//   llvm/lib/Target/Xtensa/{XtensaInstrFormats,XtensaInstrInfo,XtensaOperands}.td
//   commit f6ee8246025cea8986ce90f5fe3660efcd66cb5f
// Apache License v2.0 WITH LLVM-exception; see
//   licenses/LLVM-Apache-2.0-with-LLVM-exception.txt
//
//! Inst -> little-endian bytes. The inverse of [`crate::decode::decode`].

use crate::*;
use alloc::vec::Vec;

/// Emit `len` little-endian bytes of `w` into `out`.
#[inline]
fn emit(out: &mut Vec<u8>, w: u32, len: usize) {
    for i in 0..len {
        out.push((w >> (8 * i)) as u8);
    }
}

/// Pack a 24-bit `RRR`-family word: op0, t, s, r, op1, op2 (each masked).
#[inline]
fn pack(op0: u32, t: u32, s: u32, r: u32, op1: u32, op2: u32) -> u32 {
    (op0 & 0xf)
        | ((t & 0xf) << 4)
        | ((s & 0xf) << 8)
        | ((r & 0xf) << 12)
        | ((op1 & 0xf) << 16)
        | ((op2 & 0xf) << 20)
}

fn rrr_fields(op: AluRrr) -> (u32, u32) {
    // (op1, op2)
    match op {
        AluRrr::And => (0, 1),
        AluRrr::Or => (0, 2),
        AluRrr::Xor => (0, 3),
        AluRrr::Add => (0, 8),
        AluRrr::Addx2 => (0, 9),
        AluRrr::Addx4 => (0, 0xa),
        AluRrr::Addx8 => (0, 0xb),
        AluRrr::Sub => (0, 0xc),
        AluRrr::Subx2 => (0, 0xd),
        AluRrr::Subx4 => (0, 0xe),
        AluRrr::Subx8 => (0, 0xf),
        AluRrr::Src => (1, 8),
        AluRrr::Mul16u => (1, 0xc),
        AluRrr::Mul16s => (1, 0xd),
        AluRrr::Mull => (2, 8),
        AluRrr::Muluh => (2, 0xa),
        AluRrr::Mulsh => (2, 0xb),
        AluRrr::Quou => (2, 0xc),
        AluRrr::Quos => (2, 0xd),
        AluRrr::Remu => (2, 0xe),
        AluRrr::Rems => (2, 0xf),
        AluRrr::Min => (3, 4),
        AluRrr::Max => (3, 5),
        AluRrr::Minu => (3, 6),
        AluRrr::Maxu => (3, 7),
        AluRrr::Moveqz => (3, 8),
        AluRrr::Movnez => (3, 9),
        AluRrr::Movltz => (3, 0xa),
        AluRrr::Movgez => (3, 0xb),
    }
}

/// Encode `inst` to little-endian machine bytes.
pub fn encode(inst: &Inst) -> Vec<u8> {
    let mut out = Vec::with_capacity(3);
    match *inst {
        Inst::Rrr(op, rd, rs, rt) => {
            let (op1, op2) = rrr_fields(op);
            emit(
                &mut out,
                pack(
                    0,
                    rt.num() as u32,
                    rs.num() as u32,
                    rd.num() as u32,
                    op1,
                    op2,
                ),
                3,
            );
        }
        Inst::Rt(op, rd, rt) => {
            // rd is the r/t-dest field, rt the source. See matching decode arms.
            let w = match op {
                // RST0 op2=6: neg (s=0) / abs (s=1); r=rd, t=rt
                AluRt::Neg => pack(0, rt.num() as u32, 0, rd.num() as u32, 0, 6),
                AluRt::Abs => pack(0, rt.num() as u32, 1, rd.num() as u32, 0, 6),
                // RST1: sra (op2=0xb) / srl (op2=9); s=0; r=rd, t=rt
                AluRt::Sra => pack(0, rt.num() as u32, 0, rd.num() as u32, 1, 0xb),
                AluRt::Srl => pack(0, rt.num() as u32, 0, rd.num() as u32, 1, 9),
                // ST1 (op2=4): nsa (r=0xe) / nsau (r=0xf); operands "t, s" -> t=rd, s=rt
                AluRt::Nsa => pack(0, rd.num() as u32, rt.num() as u32, 0xe, 0, 4),
                AluRt::Nsau => pack(0, rd.num() as u32, rt.num() as u32, 0xf, 0, 4),
            };
            emit(&mut out, w, 3);
        }
        Inst::Rs(op, rd, rs) => {
            let w = match op {
                // RST1 op2=0xa: sll r, s ; t=0
                AluRs::Sll => pack(0, 0, rs.num() as u32, rd.num() as u32, 1, 0xa),
                // ST0 r=1: movsp t, s
                AluRs::Movsp => pack(0, rd.num() as u32, rs.num() as u32, 1, 0, 0),
            };
            emit(&mut out, w, 3);
        }
        Inst::ShiftSet(op, rs) => {
            let r_field = match op {
                ShiftSetOp::Ssr => 0,
                ShiftSetOp::Ssl => 1,
                ShiftSetOp::Ssa8l => 2,
                ShiftSetOp::Ssa8b => 3,
            };
            emit(&mut out, pack(0, 0, rs.num() as u32, r_field, 0, 4), 3);
        }
        Inst::Ssai(imm) => {
            let s = (imm & 0xf) as u32;
            let t = ((imm >> 4) & 0x1) as u32;
            emit(&mut out, pack(0, t, s, 4, 0, 4), 3);
        }
        Inst::Slli(rd, rs, shift) => {
            // The 5-bit field holds (32 - shift); shift 32 -> field 0.
            let field = ((32u16 - shift as u16) & 0x1f) as u32;
            let op2 = (field >> 4) & 0x1; // op2 = 0 or 1
            let t = field & 0xf;
            emit(
                &mut out,
                pack(0, t, rs.num() as u32, rd.num() as u32, 1, op2),
                3,
            );
        }
        Inst::Srli(rd, rt, sa) => {
            emit(
                &mut out,
                pack(0, rt.num() as u32, (sa & 0xf) as u32, rd.num() as u32, 1, 4),
                3,
            );
        }
        Inst::Srai(rd, rt, sa) => {
            let op2 = 2 | (((sa >> 4) & 0x1) as u32); // op2 = 2 or 3
            let s = (sa & 0xf) as u32;
            emit(
                &mut out,
                pack(0, rt.num() as u32, s, rd.num() as u32, 1, op2),
                3,
            );
        }
        Inst::Extui(rd, rt, shiftimm, maskimm) => {
            // r = rd; t = rt; s = shiftimm{3-0}; op1 = 4 | shiftimm{4}; op2 = maskimm-1
            let op1 = 4 | (((shiftimm >> 4) & 0x1) as u32);
            let s = (shiftimm & 0xf) as u32;
            let op2 = (maskimm - 1) as u32;
            emit(
                &mut out,
                pack(0, rt.num() as u32, s, rd.num() as u32, op1, op2),
                3,
            );
        }
        Inst::Sext(rd, rs, imm) => {
            let t = (imm - 7) as u32;
            emit(
                &mut out,
                pack(0, t, rs.num() as u32, rd.num() as u32, 3, 2),
                3,
            );
        }
        Inst::MovN(rt, rs) => {
            // RRRN op0=0xD, r=0, t=$t, s=$s
            emit(
                &mut out,
                narrow(0xd, rt.num() as u32, rs.num() as u32, 0),
                2,
            );
        }
        Inst::AddN(rd, rs, rt) => {
            emit(
                &mut out,
                narrow(0xa, rt.num() as u32, rs.num() as u32, rd.num() as u32),
                2,
            );
        }
        Inst::AddiN(rd, rs, imm) => {
            let t = if imm == -1 { 0 } else { (imm & 0xf) as u32 };
            emit(
                &mut out,
                narrow(0xb, t, rs.num() as u32, rd.num() as u32),
                2,
            );
        }
        Inst::Addi(rt, rs, imm) => {
            let imm8 = (imm as u32) & 0xff;
            emit(
                &mut out,
                rri8(2, rt.num() as u32, rs.num() as u32, 0xc, imm8),
                3,
            );
        }
        Inst::Addmi(rt, rs, imm) => {
            let imm8 = ((imm >> 8) as u32) & 0xff;
            emit(
                &mut out,
                rri8(2, rt.num() as u32, rs.num() as u32, 0xd, imm8),
                3,
            );
        }
        Inst::Movi(rt, imm) => {
            let v = (imm as u32) & 0xfff;
            let s = (v >> 8) & 0xf;
            let imm8 = v & 0xff;
            emit(&mut out, rri8(2, rt.num() as u32, s, 0xa, imm8), 3);
        }
        Inst::MoviN(rt, imm) => {
            let field = if imm < 0 {
                (imm + 128) as u32
            } else {
                imm as u32
            } & 0x7f;
            // RI7: imm7{3-0} -> Inst{15-12}=r ; imm7{6-4} -> Inst{6-4}=t{2-0} ; i=Inst{7}=0
            let r = field & 0xf;
            let thi = (field >> 4) & 0x7;
            // t field bits: Inst{7}=i=0 (bit3 of t), Inst{6-4}=imm7{6-4}
            let t = thi; // bit7 (i) = 0
            emit(&mut out, narrow(0xc, t, rt.num() as u32, r), 2);
        }
        Inst::Load(op, rt, rs, offset) => {
            let (r, off_field) = match op {
                LoadOp::L8ui => (0, offset),
                LoadOp::L16ui => (1, offset / 2),
                LoadOp::L32i => (2, offset / 4),
                LoadOp::L16si => (9, offset / 2),
            };
            emit(
                &mut out,
                rri8(2, rt.num() as u32, rs.num() as u32, r, off_field & 0xff),
                3,
            );
        }
        Inst::Store(op, rt, rs, offset) => {
            let (r, off_field) = match op {
                StoreOp::S8i => (4, offset),
                StoreOp::S16i => (5, offset / 2),
                StoreOp::S32i => (6, offset / 4),
            };
            emit(
                &mut out,
                rri8(2, rt.num() as u32, rs.num() as u32, r, off_field & 0xff),
                3,
            );
        }
        Inst::L32iN(rt, rs, offset) => {
            let off = (offset / 4) & 0xf;
            emit(
                &mut out,
                narrow(0x8, rt.num() as u32, rs.num() as u32, off),
                2,
            );
        }
        Inst::S32iN(rt, rs, offset) => {
            let off = (offset / 4) & 0xf;
            emit(
                &mut out,
                narrow(0x9, rt.num() as u32, rs.num() as u32, off),
                2,
            );
        }
        Inst::L32r(rt, imm16) => {
            let w = 0x1 | ((rt.num() as u32) << 4) | ((imm16 as u32) << 8);
            emit(&mut out, w, 3);
        }
        Inst::BranchRr(op, rs, rt, off) => {
            let r = match op {
                BrRr::Bnone => 0,
                BrRr::Beq => 1,
                BrRr::Blt => 2,
                BrRr::Bltu => 3,
                BrRr::Ball => 4,
                BrRr::Bbc => 5,
                BrRr::Bany => 8,
                BrRr::Bne => 9,
                BrRr::Bge => 0xa,
                BrRr::Bgeu => 0xb,
                BrRr::Bnall => 0xc,
                BrRr::Bbs => 0xd,
            };
            emit(
                &mut out,
                rri8(7, rt.num() as u32, rs.num() as u32, r, (off as u32) & 0xff),
                3,
            );
        }
        Inst::BranchRi(op, rs, val, off) => {
            let m = match op {
                BrRi::Beqi => 0,
                BrRi::Bnei => 1,
                BrRi::Blti => 2,
                BrRi::Bgei => 3,
            };
            let idx = b4const_index(val).expect("b4const value not representable") as u32;
            // op0=6, n=2, m; t nibble = (m<<2)|2 ; r = idx ; s = rs ; imm8 = off
            let tnib = (m << 2) | 0x2;
            emit(
                &mut out,
                rri8(6, tnib, rs.num() as u32, idx, (off as u32) & 0xff),
                3,
            );
        }
        Inst::BranchRiu(op, rs, val, off) => {
            let m = match op {
                BrRiu::Bltui => 2,
                BrRiu::Bgeui => 3,
            };
            let idx = b4constu_index(val).expect("b4constu value not representable") as u32;
            let tnib = (m << 2) | 0x3; // n=3
            emit(
                &mut out,
                rri8(6, tnib, rs.num() as u32, idx, (off as u32) & 0xff),
                3,
            );
        }
        Inst::BranchZ(op, rs, off) => {
            let m = match op {
                BrZ::Beqz => 0,
                BrZ::Bnez => 1,
                BrZ::Bltz => 2,
                BrZ::Bgez => 3,
            };
            // BRI12: op0=6, n=1, m; s=rs; imm12=off
            let w =
                6 | (1 << 4) | (m << 6) | ((rs.num() as u32) << 8) | (((off as u32) & 0xfff) << 12);
            emit(&mut out, w, 3);
        }
        Inst::BranchBiI(set, rs, imm, off) => {
            // op0=7; r{3-1}=3(bbci)/7(bbsi); r{0}=imm{4}; t=imm{3-0}; s=rs; imm8=off
            let hi = if set { 7 } else { 3 };
            let r = (hi << 1) | ((imm as u32 >> 4) & 0x1);
            let t = (imm as u32) & 0xf;
            emit(
                &mut out,
                rri8(7, t, rs.num() as u32, r, (off as u32) & 0xff),
                3,
            );
        }
        Inst::BranchZN(nez, rs, imm6) => {
            // op0=C, i(bit7)=1, z(bit6)=nez; imm6{3-0}->r(Inst15-12), imm6{5-4}->Inst5-4
            let r = imm6 & 0xf;
            let hi = (imm6 >> 4) & 0x3;
            // t field = Inst{7-4}: bit7=i=1, bit6=z=nez, bits5-4=imm6{5-4}
            let t = (1 << 3) | ((nez as u32) << 2) | hi;
            emit(&mut out, narrow(0xc, t, rs.num() as u32, r), 2);
        }
        Inst::J(off) => {
            let w = 6 | (((off as u32) & 0x3ffff) << 6);
            emit(&mut out, w, 3);
        }
        Inst::Jx(rs) => {
            // op0=0,op1=0,op2=0,r=0,m=2,n=2,t=0,s=rs
            let w = pack(0, 0, rs.num() as u32, 0, 0, 0) | (2 << 6) | (2 << 4);
            emit(&mut out, w, 3);
        }
        Inst::Call(op, off) => {
            let n = match op {
                CallOp::Call0 => 0,
                CallOp::Call4 => 1,
                CallOp::Call8 => 2,
                CallOp::Call12 => 3,
            };
            let w = 5 | (n << 4) | (((off as u32) & 0x3ffff) << 6);
            emit(&mut out, w, 3);
        }
        Inst::Callx(op, rs) => {
            let n = match op {
                CallxOp::Callx0 => 0,
                CallxOp::Callx4 => 1,
                CallxOp::Callx8 => 2,
                CallxOp::Callx12 => 3,
            };
            // op0=0,op1=0,op2=0,r=0,m=3,n; s=rs,t=0
            let w = pack(0, 0, rs.num() as u32, 0, 0, 0) | (3 << 6) | (n << 4);
            emit(&mut out, w, 3);
        }
        Inst::Entry(rs, imm) => {
            let imm12 = (imm >> 3) & 0xfff;
            // op0=6, n=3, m=0; s=rs; imm12
            let w = (6 | (3 << 4)) | ((rs.num() as u32) << 8) | (imm12 << 12);
            emit(&mut out, w, 3);
        }
        Inst::Nullary(op) => {
            let w = match op {
                // ST0 SYNC group: op0=0,op1=0,op2=0,r=2,s=0,t=..
                NullaryOp::Isync => pack(0, 0x0, 0, 2, 0, 0),
                NullaryOp::Rsync => pack(0, 0x1, 0, 2, 0, 0),
                NullaryOp::Esync => pack(0, 0x2, 0, 2, 0, 0),
                NullaryOp::Dsync => pack(0, 0x3, 0, 2, 0, 0),
                NullaryOp::Memw => pack(0, 0xc, 0, 2, 0, 0),
                NullaryOp::Extw => pack(0, 0xd, 0, 2, 0, 0),
                NullaryOp::Nop => pack(0, 0xf, 0, 2, 0, 0),
                // SNM0 group
                NullaryOp::Ret => pack(0, 0, 0, 0, 0, 0) | (2 << 6),
                NullaryOp::Retw => pack(0, 0, 0, 0, 0, 0) | (2 << 6) | (1 << 4),
                NullaryOp::Ill => pack(0, 0, 0, 0, 0, 0),
                // Assembler golden bytes `00 50 00`: r=5, s=0, t=0.
                NullaryOp::Syscall => pack(0, 0, 0, 5, 0, 0),
            };
            emit(&mut out, w, 3);
        }
        Inst::NullaryN(op) => {
            let w = match op {
                NullaryNarrowOp::RetN => narrow(0xd, 0, 0, 0xf),
                NullaryNarrowOp::RetwN => narrow(0xd, 1, 0, 0xf),
                NullaryNarrowOp::NopN => narrow(0xd, 3, 0, 0xf),
                NullaryNarrowOp::IllN => narrow(0xd, 6, 0, 0xf),
            };
            emit(&mut out, w, 2);
        }
    }
    out
}

/// Pack an `RRI8` word (op0, t, s, r, imm8).
#[inline]
fn rri8(op0: u32, t: u32, s: u32, r: u32, imm8: u32) -> u32 {
    (op0 & 0xf) | ((t & 0xf) << 4) | ((s & 0xf) << 8) | ((r & 0xf) << 12) | ((imm8 & 0xff) << 16)
}

/// Pack a 16-bit narrow word (op0, t, s, r).
#[inline]
fn narrow(op0: u32, t: u32, s: u32, r: u32) -> u32 {
    (op0 & 0xf) | ((t & 0xf) << 4) | ((s & 0xf) << 8) | ((r & 0xf) << 12)
}
