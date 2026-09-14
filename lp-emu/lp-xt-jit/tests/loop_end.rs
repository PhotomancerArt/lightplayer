//! Rule 6: a `loop` names `LEND` as a start and marks the instruction ending
//! exactly at `LEND` a terminator with a static back-edge to `LBEG`.
//!
//! The hazard is silent: no branch is encoded at `LEND`, so a walk that had
//! not decoded the `loop` would run the body straight through once (study
//! §2.2).

use lp_xt_inst::{Inst, LoopOp, NullaryNarrowOp, Reg};
use lp_xt_jit::blocks::BlockEnd;
use lp_xt_jit::discover::{Bounds, Extent, build, discover};

fn a(n: u8) -> Reg {
    Reg::new(n)
}

fn image(at: u32, insts: &[Inst]) -> (Vec<u8>, u32) {
    let mut bytes = Vec::new();
    for inst in insts {
        bytes.extend_from_slice(&lp_xt_inst::encode(inst));
    }
    let end = at + bytes.len() as u32;
    (bytes, end)
}

/// F+0: movi.n a3, 4 (2); F+2: loopnez a3, LEND (3); F+5: addi (3); F+8:
/// addi (3); F+11 = LEND: ret.n (2). `imm8 = LEND - (F+2+4) = 5`.
const F: u32 = 0x4000_8000;

fn program() -> (Vec<u8>, u32) {
    image(
        F,
        &[
            Inst::MoviN(a(3), 4),
            Inst::Loop(LoopOp::Loopnez, a(3), 5),
            Inst::Addi(a(2), a(2), 1),
            Inst::Addi(a(2), a(2), 1),
            Inst::NullaryN(NullaryNarrowOp::RetN),
        ],
    )
}

#[test]
fn a_loopnez_body_is_two_blocks_with_the_back_edge_on_the_last_instruction() {
    let (bytes, end) = program();
    assert_eq!(end, F + 13);
    let mut fetch = |pc: u32| {
        pc.checked_sub(F)
            .and_then(|o| bytes.get(o as usize))
            .copied()
    };
    let extents = [Extent { start: F, end }];
    let found = discover(
        &[F],
        Bounds {
            extents: &extents,
            spans: &[],
        },
        usize::MAX,
        &mut fetch,
    );
    let starts: Vec<u32> = found.set.blocks.iter().map(|b| b.pc).collect();
    assert_eq!(starts, vec![F, F + 5, F + 11]);
    let b = &found.set.blocks;
    // The head ends at the `loop`.
    assert_eq!(b[0].insts.len(), 2);
    assert!(matches!(b[0].insts[1].1.inst, Inst::Loop(..)));
    assert_eq!(b[0].end, BlockEnd::Term);
    // The body ends at `LEND`, on an instruction that is not otherwise a
    // terminator, with the back-edge to `LBEG`.
    assert_eq!(b[1].insts.len(), 2);
    let (last_pc, last) = b[1].insts[1];
    assert_eq!(last_pc, F + 8);
    assert!(matches!(last.inst, Inst::Addi(..)));
    assert!(last.control);
    assert_eq!(last.lbeg, Some(F + 5));
    assert_eq!(b[1].end, BlockEnd::Term);
    // The first body instruction carries no marker.
    assert_eq!(b[1].insts[0].1.lbeg, None);
    assert!(!b[1].insts[0].1.control);
    // After the loop.
    assert_eq!(b[2].insts.len(), 1);
    assert_eq!(found.stats.loop_ends, 1);
}

/// The supplied-starts door marks `LEND` the same way: the body's last
/// instruction is a terminator whether or not edges were followed.
#[test]
fn the_supplied_starts_door_marks_lend_too() {
    let (bytes, _) = program();
    let mut fetch = |pc: u32| {
        pc.checked_sub(F)
            .and_then(|o| bytes.get(o as usize))
            .copied()
    };
    let found = build(&[F, F + 5], &mut fetch);
    let starts: Vec<u32> = found.set.blocks.iter().map(|b| b.pc).collect();
    assert_eq!(starts, vec![F, F + 5]);
    assert_eq!(found.set.blocks[1].insts.len(), 2);
    assert_eq!(found.set.blocks[1].insts[1].1.lbeg, Some(F + 5));
    assert_eq!(found.set.blocks[1].end, BlockEnd::Term);
    assert_eq!(found.stats.loop_ends, 1);
}
