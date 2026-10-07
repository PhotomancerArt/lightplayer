//! A synthetic network-seam guest: a core page whose table carries the nine
//! `net_*` calls (the engaged byte on `net_mac`'s entry alone, as the firmware
//! lays it out), and a **command loop** a test drives one call at a time —
//! so every answer is reached through the real path (the patched entry, the
//! `ebreak`, the answer, `ret`) on a machine that runs no firmware.
//!
//! ```text
//! 0x4200_0040  the seam table (self = 0x4200_0040)
//! 0x4200_0400  net_mac's engaged byte (0)
//! 0x4200_1200  ws281x_wait_step (only with `led`)
//! 0x4200_1400  net_mac, then each call 0x40 apart, in `lp_seam::net::CALLS`
//!              order: addi zero,zero,<hint>; li a0,0; ret (silicon's body)
//! 0x4200_2000  main: the command loop (or, `sleeper`, `wfi` forever)
//! 0x4200_3000  the wake handler: clear the line, swap the pending word,
//!              OR what it saw into WOKEN
//! ```
//!
//! The command block in RAM: the host writes `a0..a3`, then the function's
//! address to `CMD`; the loop calls it, stores `a0` to `RESULT`, clears `CMD`
//! and counts `DONE`. The handler touches only `s5..s7`, which the loop never
//! uses, so it may land anywhere in it.

use lp_emu_esp_common::seam::net::{LanConfig, VirtualAccessPoint, VirtualLan};
use lp_emu_esp32c6::machine::{Esp32C6Machine, StopCondition};
use lp_seam::SeamDecl;
use lp_seam::table::*;

use super::reg::*;
use super::*;

/// `net_mac`'s engaged byte.
pub const NET_ENGAGED: u32 = VBASE + 0x400;
/// The first network call's entry; the rest follow [`NET_STRIDE`] apart.
pub const NET_FNS: u32 = VBASE + 0x1400;
pub const NET_STRIDE: u32 = 0x40;

/// The command block.
pub const CMD: u32 = RAM + 0x40;
pub const ARGS: u32 = CMD + 4;
pub const RESULT: u32 = CMD + 20;
pub const DONE: u32 = CMD + 24;
/// Buffers a test hands to a call: a frame's, a scan's, a name's.
pub const BUF: u32 = RAM + 0x1000;
pub const BUF2: u32 = RAM + 0x2000;
pub const BUF3: u32 = RAM + 0x2100;

const A3: u32 = 13;
const S5: u32 = 21;
const S6: u32 = 22;
const S7: u32 = 23;
/// `INTPRI.cpu_intr_from_cpu_3`.
const FROM_CPU3: u32 = 0x600c_5000 + 0x9c;

/// One call's entry address.
pub fn net_fn(decl: &SeamDecl) -> u32 {
    let i = lp_seam::net::CALLS
        .iter()
        .position(|d| d.id == decl.id)
        .expect("a network call");
    NET_FNS + NET_STRIDE * i as u32
}

/// What a network core page carries besides the nine calls.
#[derive(Clone, Copy, Default)]
pub struct NetPage {
    /// The wake pending word, 0 for none.
    pub pending: u32,
    /// `ws281x_wait_step` too, so `led=fast` can engage beside the network.
    pub led: bool,
    /// Leave this call's entry out (an image whose table is incomplete).
    pub without: Option<u16>,
    /// `main` sleeps in `wfi` forever (interrupts on) instead of running the
    /// command loop: only the wake handler ever runs.
    pub sleeper: bool,
}

/// The page: the table, the calls, the command loop and the wake handler.
pub fn net_core_page(spec: NetPage) -> Vec<u8> {
    let mut page = vec![0u8; PAGE];
    let mut entries: Vec<(u16, u8, u8, u32, u32)> = Vec::new();
    if spec.led {
        entries.push((lp_seam::ws281x_wait_step::ID, 1, 1, WAIT, 0));
    }
    for (i, d) in lp_seam::net::CALLS.iter().enumerate() {
        if spec.without == Some(d.id) {
            continue;
        }
        let engaged = if i == 0 { NET_ENGAGED } else { 0 };
        entries.push((d.id, d.kind as u8, d.shape as u8, net_fn(d), engaged));
    }
    let t = (TABLE - VBASE) as usize;
    page[t..t + 16].copy_from_slice(&MAGIC);
    page[t + OFFSET_ABI..t + OFFSET_ABI + 8].copy_from_slice(&lp_seam::SEAM_ABI_ID.to_le_bytes());
    page[t + OFFSET_VERSION..t + OFFSET_VERSION + 5].copy_from_slice(b"guest");
    page[t + OFFSET_SELF..t + OFFSET_SELF + 4].copy_from_slice(&TABLE.to_le_bytes());
    page[t + OFFSET_COUNT..t + OFFSET_COUNT + 4]
        .copy_from_slice(&(entries.len() as u32).to_le_bytes());
    page[t + OFFSET_PENDING..t + OFFSET_PENDING + 4].copy_from_slice(&spec.pending.to_le_bytes());
    for (i, (id, kind, shape, f, e)) in entries.iter().enumerate() {
        let at = t + OFFSET_ENTRIES + i * ENTRY_LEN;
        page[at..at + 2].copy_from_slice(&id.to_le_bytes());
        page[at + 2] = *kind;
        page[at + 3] = *shape;
        page[at + 4..at + 8].copy_from_slice(&f.to_le_bytes());
        page[at + 8..at + 12].copy_from_slice(&e.to_le_bytes());
    }
    put(&mut page, WAIT, &[addi(0, 0, 1), jalr(0, 1, 0)]);
    for d in lp_seam::net::CALLS {
        put(
            &mut page,
            net_fn(d),
            &[addi(0, 0, d.hint()), addi(A0, 0, 0), jalr(0, RA, 0)],
        );
    }
    let main = if spec.sleeper {
        sleeper()
    } else {
        command_loop()
    };
    put(&mut page, MAIN, &main);
    put(&mut page, ISR, &wake_handler());
    page
}

fn command_loop() -> Vec<u32> {
    let mut m = Vec::new();
    m.extend(li(S0, CMD));
    m.push(addi(T0, 0, 8)); // mstatus.MIE
    m.push(csrs(0x300, T0));
    let top = m.len();
    m.push(lw(T1, S0, 0));
    m.push(beq(T1, 0, -4));
    m.push(lw(A0, S0, 4));
    m.push(lw(A1, S0, 8));
    m.push(lw(A2, S0, 12));
    m.push(lw(A3, S0, 16));
    m.push(jalr(RA, T1, 0));
    m.push(sw(A0, S0, 20));
    m.push(sw(0, S0, 0));
    m.push(lw(T2, S0, 24));
    m.push(addi(T2, T2, 1));
    m.push(sw(T2, S0, 24));
    let back = -(4 * (m.len() - top) as i32);
    m.push(jal(0, back));
    m
}

fn sleeper() -> Vec<u32> {
    vec![addi(T0, 0, 8), csrs(0x300, T0), wfi(), jal(0, -4)]
}

fn wake_handler() -> Vec<u32> {
    let mut isr = Vec::new();
    isr.extend(li(S5, FROM_CPU3));
    isr.push(sw(0, S5, 0)); // the line, first
    isr.extend(li(S5, PENDING));
    isr.push(amoswap_w(S6, S5, 0)); // then the word
    isr.extend(li(S5, WOKEN));
    isr.push(lw(S7, S5, 0));
    isr.push(or(S7, S7, S6));
    isr.push(sw(S7, S5, 0));
    isr.push(mret());
    isr
}

/// Ask the loop to call `decl` with `args`; it runs on the machine's next
/// slice. For a host that drives the machine itself (a lockstep runner).
pub fn post(m: &mut Esp32C6Machine, decl: &SeamDecl, args: [u32; 4]) -> u32 {
    let done = m.peek_word(DONE).unwrap();
    for (i, a) in args.iter().enumerate() {
        assert!(m.poke_word(ARGS + 4 * i as u32, *a));
    }
    assert!(m.poke_word(CMD, net_fn(decl)));
    done
}

/// The answer to the call [`post`] asked for, once the loop has made it.
pub fn answered(m: &mut Esp32C6Machine, done_before: u32) -> Option<u32> {
    (m.peek_word(DONE).unwrap() != done_before).then(|| m.peek_word(RESULT).unwrap())
}

/// Make one call and return its answer, running the machine a little at a
/// time until the loop has made it.
pub fn call(m: &mut Esp32C6Machine, decl: &SeamDecl, args: [u32; 4]) -> u32 {
    let done = post(m, decl, args);
    for _ in 0..1_000 {
        run_for(m, 200);
        if let Some(answer) = answered(m, done) {
            return answer;
        }
    }
    panic!("the command loop never made the call to {}", decl.name);
}

/// Run the machine `cycles` more guest cycles.
pub fn run_for(m: &mut Esp32C6Machine, cycles: u64) {
    let stop = StopCondition {
        stop_cycle: Some(m.cycles() + cycles),
        ..StopCondition::default()
    };
    m.run_until(&stop);
}

/// Write `bytes` at `at` in guest RAM, word by word (`at` 4-aligned).
pub fn poke_bytes(m: &mut Esp32C6Machine, at: u32, bytes: &[u8]) {
    for (i, chunk) in bytes.chunks(4).enumerate() {
        let mut w = [0u8; 4];
        w[..chunk.len()].copy_from_slice(chunk);
        assert!(m.poke_word(at + 4 * i as u32, u32::from_le_bytes(w)));
    }
}

/// `len` bytes of guest RAM at `at` (4-aligned).
pub fn peek_bytes(m: &mut Esp32C6Machine, at: u32, len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..len.div_ceil(4) {
        out.extend_from_slice(&m.peek_word(at + 4 * i as u32).unwrap().to_le_bytes());
    }
    out.truncate(len);
    out
}

/// `net_connect(name, password)` through the loop, the strings placed in RAM.
pub fn connect(m: &mut Esp32C6Machine, name: &[u8], password: &[u8]) -> u32 {
    poke_bytes(m, BUF2, name);
    poke_bytes(m, BUF3, password);
    call(
        m,
        &lp_seam::net_connect::DECL,
        [BUF2, name.len() as u32, BUF3, password.len() as u32],
    )
}

/// The networks the tests hear: the same test values as
/// `lp-emu-esp-common/testdata/virtual_lan.toml` (a mesh named `home` at two
/// strengths, an open `cafe`, a hidden `attic`). Never real credentials.
pub fn fixture_lan() -> VirtualLan {
    VirtualLan::new(LanConfig::new(160))
        .with_access_point(VirtualAccessPoint::secured("home", "test-password-1", -45))
        .with_access_point(VirtualAccessPoint::secured("home", "test-password-1", -70))
        .with_access_point(VirtualAccessPoint::open("cafe", -80))
        .with_access_point(VirtualAccessPoint::secured("attic", "test-password-2", -60).hidden())
}

/// The fixture's password for `home`.
pub const HOME_PASSWORD: &[u8] = b"test-password-1";
