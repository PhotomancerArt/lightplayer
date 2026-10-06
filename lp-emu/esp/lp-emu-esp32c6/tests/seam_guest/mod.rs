//! A synthetic seam guest: a hand-built "core" in flash, with a seam table
//! and seam functions, mapped through the cache MMU the way a bootloader
//! would — so the scan, the live-MMU arming and the answers are exercised on
//! a machine that runs no firmware at all.
//!
//! One 64 KiB page, linked at the window base:
//!
//! ```text
//! 0x4200_0040  the seam table (self = 0x4200_0040)
//! 0x4200_0400  test_take's engaged byte (0)
//! 0x4200_1000  test_echo:        addi zero,zero,0x701; xor a0,a0,a1; xor a0,a0,a2; ret
//! 0x4200_1100  test_take:        addi zero,zero,0x702; li a0,0; ret
//! 0x4200_1200  ws281x_wait_step: addi zero,zero,1; ret
//! 0x4200_2000  main (the test's own)
//! 0x4200_3000  an interrupt handler (the test's own, optional)
//! ```
//!
//! Shared by the `seam_*` gate tests; each test file says what it proves.

#![allow(dead_code, reason = "each seam test uses part of the shared guest")]

use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine};
use lp_seam::table::*;

pub const VBASE: u32 = 0x4200_0000;
pub const TABLE: u32 = VBASE + 0x40;
pub const ENGAGED: u32 = VBASE + 0x400;
pub const ECHO: u32 = VBASE + 0x1000;
pub const TAKE: u32 = VBASE + 0x1100;
pub const WAIT: u32 = VBASE + 0x1200;
pub const MAIN: u32 = VBASE + 0x2000;
pub const ISR: u32 = VBASE + 0x3000;
pub const PAGE: usize = 0x1_0000;
/// Two cores, as a chip holds after an update.
pub const CORE_A: u32 = 0x1_0000;
pub const CORE_B: u32 = 0x2_0000;
pub const CHIP: usize = 4 * 1024 * 1024;
/// Scratch RAM a guest program may use.
pub const RAM: u32 = 0x4081_0000;

/// What a core page's table names.
#[derive(Clone, Copy)]
pub struct Tables {
    pub echo: bool,
    pub take: bool,
    pub wait: bool,
    /// The wake pending word's address, 0 for none.
    pub pending: u32,
}

impl Tables {
    pub const ALL: Tables = Tables {
        echo: true,
        take: true,
        wait: true,
        pending: 0,
    };
}

/// One 64 KiB core page: the table, the seam functions, `main` and an
/// optional interrupt handler.
pub fn core_page(tables: Tables, main: &[u32], isr: &[u32]) -> Vec<u8> {
    let mut page = vec![0u8; PAGE];
    let mut entries: Vec<(u16, u8, u8, u32, u32)> = Vec::new();
    if tables.wait {
        entries.push((lp_seam::ws281x_wait_step::ID, 1, 1, WAIT, 0));
    }
    if tables.echo {
        entries.push((lp_seam::test_echo::ID, 1, 1, ECHO, 0));
    }
    if tables.take {
        entries.push((lp_seam::test_take::ID, 2, 2, TAKE, ENGAGED));
    }
    let t = (TABLE - VBASE) as usize;
    page[t..t + 16].copy_from_slice(&MAGIC);
    page[t + OFFSET_ABI..t + OFFSET_ABI + 8].copy_from_slice(&lp_seam::SEAM_ABI_ID.to_le_bytes());
    page[t + OFFSET_VERSION..t + OFFSET_VERSION + 5].copy_from_slice(b"guest");
    page[t + OFFSET_SELF..t + OFFSET_SELF + 4].copy_from_slice(&TABLE.to_le_bytes());
    page[t + OFFSET_COUNT..t + OFFSET_COUNT + 4]
        .copy_from_slice(&(entries.len() as u32).to_le_bytes());
    page[t + OFFSET_PENDING..t + OFFSET_PENDING + 4].copy_from_slice(&tables.pending.to_le_bytes());
    for (i, (id, kind, shape, f, e)) in entries.iter().enumerate() {
        let at = t + OFFSET_ENTRIES + i * ENTRY_LEN;
        page[at..at + 2].copy_from_slice(&id.to_le_bytes());
        page[at + 2] = *kind;
        page[at + 3] = *shape;
        page[at + 4..at + 8].copy_from_slice(&f.to_le_bytes());
        page[at + 8..at + 12].copy_from_slice(&e.to_le_bytes());
    }
    put(
        &mut page,
        ECHO,
        &[
            addi(0, 0, 0x701),
            xor(10, 10, 11),
            xor(10, 10, 12),
            jalr(0, 1, 0),
        ],
    );
    put(
        &mut page,
        TAKE,
        &[addi(0, 0, 0x702), addi(10, 0, 0), jalr(0, 1, 0)],
    );
    put(&mut page, WAIT, &[addi(0, 0, 1), jalr(0, 1, 0)]);
    put(&mut page, MAIN, main);
    put(&mut page, ISR, isr);
    page
}

/// A whole chip of `0xff` holding each `(offset, page)`.
pub fn chip(cores: &[(u32, &[u8])]) -> Vec<u8> {
    let mut flash = vec![0xffu8; CHIP];
    for (at, page) in cores {
        flash[*at as usize..*at as usize + page.len()].copy_from_slice(page);
    }
    flash
}

/// A bare machine over `flash`, built with `builder`'s other settings.
pub fn machine(flash: Vec<u8>, builder: Esp32C6Builder) -> Esp32C6Machine {
    builder
        .flash(FlashBacking::Bytes(flash))
        .build()
        .expect("a bare machine builds")
}

/// What a bootloader does before it jumps: map `core` at the window base,
/// fill the window, put the hart at `main`; then the app has started.
pub fn boot_core(m: &mut Esp32C6Machine, core: u32) {
    assert!(m.cache().lock().unwrap().map(VBASE, core), "maps");
    let (flash, cache) = (m.flash().clone(), m.cache().clone());
    lp_emu_esp32c6::cache::fill(&mut m.bus, &flash, &cache);
    m.harts[0].set_pc(MAIN);
    m.seams_start_if_app_running();
}

// ---- the wake consumer guest ---------------------------------------------

/// The wake pending word the table names (guest RAM).
pub const PENDING: u32 = RAM;
/// Bits the guest's handler saw, for its main loop.
pub const WOKEN: u32 = RAM + 4;
/// Where the main loop appends what each take returned.
pub const LOG: u32 = RAM + 0x100;
/// The CPU interrupt line the host routes `FROM_CPU_INTR3` to.
pub const WAKE_LINE: u32 = 10;
/// `INTPRI.cpu_intr_from_cpu_3`.
const FROM_CPU3: u32 = 0x600c_5000 + 0x9c;

/// How the consumer's main loop waits between wakes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Idle {
    /// `wfi` with interrupts masked, then unmask: a real consumer's sleep.
    Wfi,
    /// Spin: a render loop that never sleeps.
    Busy,
}

/// A wake consumer: the handler clears the line, then swaps the pending word
/// to zero and ORs what it saw into [`WOKEN`]; the main loop swaps
/// [`WOKEN`] to zero and, when anything was set, drains `test_take(0, …)`
/// into [`LOG`] until a take returns 0. `s4` counts main-loop passes (the
/// "render" progress a flood must not starve); `s0` is the log's cursor.
pub fn wake_consumer(idle: Idle) -> (Vec<u32>, Vec<u32>) {
    use reg::*;
    const T3: u32 = 28;
    let mut isr = Vec::new();
    isr.extend(li(T1, FROM_CPU3));
    isr.push(sw(0, T1, 0)); // the line, first
    isr.extend(li(T1, PENDING));
    isr.push(amoswap_w(T2, T1, 0)); // then the word
    isr.extend(li(T1, WOKEN));
    isr.push(lw(T3, T1, 0));
    isr.push(or(T3, T3, T2));
    isr.push(sw(T3, T1, 0));
    isr.push(mret());

    let mut main = Vec::new();
    main.extend(li(S3, WOKEN));
    main.extend(li(S0, LOG));
    main.push(addi(S4, 0, 0));
    main.push(addi(T0, 0, 8)); // mstatus.MIE
    main.push(csrs(0x300, T0));
    let at = |m: &Vec<u32>| MAIN + 4 * m.len() as u32;
    let top = at(&main);
    main.push(addi(S4, S4, 1));
    main.push(csrc(0x300, T0));
    main.push(amoswap_w(S1, S3, 0));
    let to_work = main.len();
    main.push(0); // bne s1, zero, work — patched below
    if idle == Idle::Wfi {
        main.push(wfi());
    }
    main.push(csrs(0x300, T0));
    let j = at(&main);
    main.push(jal(0, top.wrapping_sub(j) as i32));
    let work = at(&main);
    main[to_work] = bne(S1, 0, work.wrapping_sub(MAIN + 4 * to_work as u32) as i32);
    main.push(csrs(0x300, T0));
    let drain = at(&main);
    main.push(addi(A0, 0, 0));
    main.push(addi(A1, S0, 0));
    main.push(addi(A2, 0, 64));
    let call = at(&main);
    main.push(jal(RA, TAKE.wrapping_sub(call) as i32));
    let back = at(&main);
    main.push(beq(A0, 0, top.wrapping_sub(back) as i32));
    main.push(add(S0, S0, A0));
    let j = at(&main);
    main.push(jal(0, drain.wrapping_sub(j) as i32));
    (main, isr)
}

/// Route `FROM_CPU_INTR3` to [`WAKE_LINE`] at priority 1 and point the
/// hart's direct-mode trap vector at [`ISR`] — what the firmware's wake
/// handler would bind at start-up.
pub fn bind_wake(m: &mut Esp32C6Machine) {
    let source = u32::from(lp_seam::wake::WAKE_SOURCE_ESP32C6);
    assert!(m.poke_word(0x6001_0000 + 4 * source, WAKE_LINE));
    let enable = m.peek_word(0x2000_1000).unwrap_or(0);
    assert!(m.poke_word(0x2000_1000, enable | (1 << WAKE_LINE)));
    assert!(m.poke_word(
        0x2000_1010 + 4 * WAKE_LINE,
        u32::from(lp_seam::wake::WAKE_PRIORITY)
    ));
    let mie = m.harts[0].csr().mie;
    assert!(m.harts[0].set_csr_raw(0x305, ISR));
    assert!(m.harts[0].set_csr_raw(0x304, mie | (1 << WAKE_LINE)));
}

/// What the guest's log holds: the `u32` sequence numbers it took, in order.
pub fn logged(m: &mut Esp32C6Machine) -> Vec<u32> {
    let end = m.harts[0].regs()[reg::S0 as usize] as u32;
    (LOG..end)
        .step_by(4)
        .map(|at| m.peek_word(at).expect("guest RAM"))
        .collect()
}

/// Place `words` at `vaddr` in a page linked at [`VBASE`].
pub fn put(page: &mut [u8], vaddr: u32, words: &[u32]) {
    let at = (vaddr - VBASE) as usize;
    for (i, w) in words.iter().enumerate() {
        page[at + 4 * i..at + 4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
}

// ---- RV32 encoders (full-width only; enough for these guests) ------------

pub fn lui(rd: u32, imm: u32) -> u32 {
    (imm & 0xffff_f000) | (rd << 7) | 0x37
}
pub fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}
pub fn xor(rd: u32, rs1: u32, rs2: u32) -> u32 {
    (rs2 << 20) | (rs1 << 15) | (0b100 << 12) | (rd << 7) | 0x33
}
/// `lui` + `addi`, any 32-bit value.
pub fn li(rd: u32, value: u32) -> [u32; 2] {
    let lo = (value & 0xfff) as i32;
    let lo = if lo >= 0x800 { lo - 0x1000 } else { lo };
    let hi = value.wrapping_sub(lo as u32);
    [lui(rd, hi), addi(rd, rd, lo)]
}
pub fn jal(rd: u32, offset: i32) -> u32 {
    let o = offset as u32;
    (((o >> 20) & 1) << 31)
        | (((o >> 1) & 0x3ff) << 21)
        | (((o >> 11) & 1) << 20)
        | (((o >> 12) & 0xff) << 12)
        | (rd << 7)
        | 0x6f
}
pub fn jalr(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x67
}
fn branch(funct3: u32, rs1: u32, rs2: u32, offset: i32) -> u32 {
    let o = offset as u32;
    (((o >> 12) & 1) << 31)
        | (((o >> 5) & 0x3f) << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (funct3 << 12)
        | (((o >> 1) & 0xf) << 8)
        | (((o >> 11) & 1) << 7)
        | 0x63
}
pub fn beq(rs1: u32, rs2: u32, offset: i32) -> u32 {
    branch(0b000, rs1, rs2, offset)
}
pub fn bne(rs1: u32, rs2: u32, offset: i32) -> u32 {
    branch(0b001, rs1, rs2, offset)
}
pub fn lbu(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (0b100 << 12) | (rd << 7) | 0x03
}
pub fn lw(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (0b010 << 12) | (rd << 7) | 0x03
}
pub fn sw(rs2: u32, rs1: u32, imm: i32) -> u32 {
    let i = imm as u32 & 0xfff;
    ((i >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | (0b010 << 12) | ((i & 0x1f) << 7) | 0x23
}
pub fn add(rd: u32, rs1: u32, rs2: u32) -> u32 {
    (rs2 << 20) | (rs1 << 15) | (rd << 7) | 0x33
}
pub fn or(rd: u32, rs1: u32, rs2: u32) -> u32 {
    (rs2 << 20) | (rs1 << 15) | (0b110 << 12) | (rd << 7) | 0x33
}
/// `amoswap.w rd, rs2, (rs1)`.
pub fn amoswap_w(rd: u32, rs1: u32, rs2: u32) -> u32 {
    (0b00001 << 27) | (rs2 << 20) | (rs1 << 15) | (0b010 << 12) | (rd << 7) | 0x2f
}
pub fn mret() -> u32 {
    0x3020_0073
}
pub fn wfi() -> u32 {
    0x1050_0073
}
/// `csrrs zero, csr, rs1` (set bits).
pub fn csrs(csr: u32, rs1: u32) -> u32 {
    (csr << 20) | (rs1 << 15) | (0b010 << 12) | 0x73
}
/// `csrrc zero, csr, rs1` (clear bits).
pub fn csrc(csr: u32, rs1: u32) -> u32 {
    (csr << 20) | (rs1 << 15) | (0b011 << 12) | 0x73
}
/// `j .`
pub fn spin() -> u32 {
    jal(0, 0)
}

/// The register numbers the guests use.
pub mod reg {
    pub const RA: u32 = 1;
    pub const SP: u32 = 2;
    pub const T0: u32 = 5;
    pub const T1: u32 = 6;
    pub const T2: u32 = 7;
    pub const S0: u32 = 8;
    pub const S1: u32 = 9;
    pub const A0: u32 = 10;
    pub const A1: u32 = 11;
    pub const A2: u32 = 12;
    pub const S2: u32 = 18;
    pub const S3: u32 = 19;
    pub const S4: u32 = 20;
}
