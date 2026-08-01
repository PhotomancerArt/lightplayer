//! Deliberate panic — proves the SYS_PANIC trap reports the message and the
//! host terminates the run with the panic exit code. Prints once first so the
//! output channel is also verified on the panic path.
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

fn main(arg: u32) -> u32 {
    println!("before_panic");
    if arg < 100 {
        panic!("boom: arg={}", arg);
    }
    0
}
emu_main!(main);
