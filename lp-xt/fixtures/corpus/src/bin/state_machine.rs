//! A small byte-classifier state machine over static data (.rodata access +
//! data-dependent branching).
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

const INPUT: &[u8] = b"let x1 = 42; if x1 > 9 { emit(x1, 0xFF); } // done 2026";

#[derive(Clone, Copy, PartialEq)]
enum State {
    Space,
    Word,
    Number,
}

fn main(_arg: u32) -> u32 {
    let mut state = State::Space;
    let mut words = 0u32;
    let mut numbers = 0u32;
    let mut transitions = 0u32;
    for &b in INPUT {
        let next = if b.is_ascii_alphabetic() || b == b'_' {
            State::Word
        } else if b.is_ascii_digit() {
            if state == State::Word {
                State::Word
            } else {
                State::Number
            }
        } else {
            State::Space
        };
        if next != state {
            transitions += 1;
            match next {
                State::Word => words += 1,
                State::Number => numbers += 1,
                State::Space => {}
            }
        }
        state = next;
    }
    println!("words={}", words);
    println!("numbers={}", numbers);
    println!("transitions={}", transitions);
    println!("len={}", INPUT.len());
    0
}
emu_main!(main);
