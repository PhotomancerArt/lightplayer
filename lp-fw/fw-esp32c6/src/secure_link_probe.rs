//! What lp-link's `secure` feature costs on this chip (feature
//! `diag_secure_link`, never shipped; plan
//! `lp2025/2026-10-01-1843-secure-link`, P5).
//!
//! The product image with a secure link linked in: after the server is up
//! and the boot project loaded, two secure links (a responder, as the device
//! will hold one, and an initiator) are wired back to back in RAM, entropy
//! from the chip's RNG, and this logs `[SECURE]` lines:
//!
//! - RAM: the responder's `Link::ram_bound_secure` beside the plain
//!   `ram_bound` for the same config, its `ram_bytes`, and the heap the
//!   pair took;
//! - three full handshakes (both ends' compute, msg1 to both up), in PMU
//!   cycles: at `lp-emu:esp32c6:t1` a cycle is an instruction, so there they
//!   are instruction counts, never time;
//! - a 64 B and a 2 KB message, 20 times each, through the secure pair and
//!   through a plain pair: the difference is what sealing costs;
//! - the main-task stack one handshake uses (painted below `sp`, scanned
//!   after; an interrupt landing meanwhile can only make it larger): the
//!   whole link path (both ends' SYN handling, the lookup answer, frames
//!   copied out), and the Noise core alone (`secure_channel`, both roles).
//!
//! The responder stays alive in a static, so the image keeps its code. A
//! `test_*` feature would not do: any such feature replaces the product with
//! a harness (`build.rs`), and the point is the product image's delta.

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, Ordering};

use lp_link::secure_channel::{KeyId, Psk, SecureEvent, SecureRole};
use lp_link::{CH_PROTO, Link, LinkConfig, LinkEvent, LinkState, Micros, NoArq};

use crate::board::esp32c6::cycle_counter;

/// `ws()` with the board's buffer cut (the USB link's: `UsbLinkShared::config`),
/// the shape a device's secure network link starts from in M6.
fn config() -> LinkConfig {
    let board = fw_esp32_common::usb_link::UsbLinkShared::config();
    LinkConfig {
        max_message: board.max_message,
        send_budget: board.send_budget,
        rx_budget: board.rx_budget,
        keep_reassembly: board.keep_reassembly,
        send_queue: board.send_queue,
        datagram_queue: board.datagram_queue,
        ..LinkConfig::ws()
    }
}

type L = Link<NoArq>;

static KEEP: AtomicPtr<L> = AtomicPtr::new(core::ptr::null_mut());

const KEY: KeyId = KeyId([0x5a; 16]);

fn psk() -> Psk {
    Psk::new([0xa5; 32])
}

fn entropy(buf: &mut [u8]) {
    esp_hal::rng::Rng::new().read(buf);
}

/// Run every measurement once and log it.
pub fn run() {
    cycle_counter::setup();
    let cfg = config();
    esp_println::println!(
        "[SECURE] ram_bound plain {} B secure {} B (ws with the board cut, no-ARQ, 32-bit)",
        L::ram_bound(&cfg),
        L::ram_bound_secure(&cfg)
    );

    let heap_before = esp_alloc::HEAP.used();
    let plain = L::new(cfg.clone(), nonce());
    let heap_plain = esp_alloc::HEAP.used();
    esp_println::println!(
        "[SECURE] heap: plain link {} B (ram_bytes {} B)",
        heap_plain.saturating_sub(heap_before),
        plain.ram_bytes()
    );
    drop(plain);
    let heap_before = esp_alloc::HEAP.used();
    let board = Box::new(L::new_secure(
        cfg.clone(),
        nonce(),
        SecureRole::Responder,
        entropy,
    ));
    let heap_board = esp_alloc::HEAP.used();
    esp_println::println!(
        "[SECURE] heap: responder link {} B (ram_bytes {} B)",
        heap_board.saturating_sub(heap_before),
        board.ram_bytes()
    );
    let mut board = board;
    let mut host = L::new_secure(
        cfg.clone(),
        nonce(),
        SecureRole::Initiator {
            key_id: KEY,
            psk: psk(),
        },
        entropy,
    );

    for i in 1..=3 {
        if i > 1 {
            host.restart(0);
        }
        let (cycles, frames) = handshake(&mut host, &mut board);
        esp_println::println!(
            "[SECURE] handshake {i}: {cycles} cycles (both ends, {frames} frames), up {}",
            host.state() == LinkState::Established && board.state() == LinkState::Established
        );
    }

    let stack = measure_stack(|| {
        host.restart(0);
        handshake(&mut host, &mut board);
    });
    esp_println::println!(
        "[SECURE] stack: one handshake through the links used {stack} B of the main stack"
    );
    let core = measure_stack(noise_core_handshake);
    esp_println::println!(
        "[SECURE] stack: the Noise core alone (both roles, one handshake) used {core} B"
    );

    let mut plain_host = L::new(cfg.clone(), nonce());
    let mut plain_board = L::new(cfg, nonce());
    shuttle(&mut plain_host, &mut plain_board, 0);
    for size in [64usize, 2048] {
        let secure = messages(&mut host, &mut board, size, 20);
        let plain = messages(&mut plain_host, &mut plain_board, size, 20);
        esp_println::println!(
            "[SECURE] {size} B x20: secure {secure} cycles, plain {plain} cycles, sealing {} cycles/message",
            secure.saturating_sub(plain) / 20
        );
    }
    esp_println::println!(
        "[SECURE] counters: handshakes {} seal_failures {} replays {}",
        board.counters().handshakes,
        board.counters().seal_failures,
        board.counters().replays
    );
    drop(host);
    KEEP.store(Box::into_raw(board), Ordering::Relaxed);
}

/// msg1 to both up: cycles and frames.
#[inline(never)]
fn handshake(host: &mut L, board: &mut L) -> (u32, u32) {
    let start = cycle_counter::read();
    let frames = shuttle(host, board, 0);
    (cycle_counter::read().wrapping_sub(start), frames)
}

/// One NNpsk0 handshake through `secure_channel` alone: the initiator's
/// msg1, the responder's read and msg2, the initiator's read.
#[inline(never)]
fn noise_core_handshake() {
    use lp_link::secure_channel::{Initiator, MSG2_LEN, Responder, prologue};
    let mut e = [0u8; 32];
    entropy(&mut e);
    let p = prologue(&KEY, nonce());
    let init = Initiator::new(&p, &psk(), e);
    entropy(&mut e);
    if let Ok(ready) = Responder::new(&p).read_msg1(init.msg1(), &psk()) {
        let mut msg2 = [0u8; MSG2_LEN];
        if ready.write_msg2(e, &[1, 2, 3, 4], &mut msg2).is_ok() {
            let mut payload = [0u8; 4];
            let ok = init.read_msg2(&msg2, &mut payload).is_ok();
            core::hint::black_box(ok);
        }
    }
}

/// The stack `f` takes below the caller's frame.
#[inline(never)]
fn measure_stack(f: impl FnOnce()) -> usize {
    const SPAN: usize = 12 * 1024;
    const PATTERN: u32 = 0x5EC0_5EC0;
    let sp: usize;
    // SAFETY: reads the stack pointer; touches no memory.
    unsafe { core::arch::asm!("mv {0}, sp", out(reg) sp) };
    let top = sp - 64;
    let bottom = top - SPAN;
    let mut addr = bottom;
    while addr < top {
        // SAFETY: `bottom..top` lies below this function's live frame, in
        // the main stack's unused part (it is 72 KB and the boot task is
        // shallow here); nothing else writes there.
        unsafe { (addr as *mut u32).write_volatile(PATTERN) };
        addr += 4;
    }
    f();
    let mut lowest = top;
    let mut addr = bottom;
    while addr < top {
        // SAFETY: as above.
        if unsafe { (addr as *const u32).read_volatile() } != PATTERN {
            lowest = addr;
            break;
        }
        addr += 4;
    }
    sp - lowest
}

/// `count` messages of `size` bytes host → board, through to delivery.
#[inline(never)]
fn messages(host: &mut L, board: &mut L, size: usize, count: usize) -> u32 {
    let payload: Vec<u8> = (0..size).map(|i| i as u8).collect();
    let start = cycle_counter::read();
    for _ in 0..count {
        let _ = host.send(CH_PROTO, &payload);
        shuttle(host, board, 0);
    }
    cycle_counter::read().wrapping_sub(start)
}

/// Move every frame each way (answering the board's key lookup) until both
/// ends are quiet; frames moved.
fn shuttle(host: &mut L, board: &mut L, now: Micros) -> u32 {
    let mut frames = 0;
    for _ in 0..64 {
        let mut moved = false;
        while let Some(f) = host.poll_transmit(now) {
            let f = f.to_vec();
            board.on_datagram(now, &f);
            frames += 1;
            moved = true;
        }
        while let Some(ev) = board.poll_secure_event() {
            if let SecureEvent::KeyLookup { key_id } = ev {
                board.provide_keys(key_id, &[psk()]);
            }
        }
        while let Some(f) = board.poll_transmit(now) {
            let f = f.to_vec();
            host.on_datagram(now, &f);
            frames += 1;
            moved = true;
        }
        while let Some(ev) = host.recv() {
            drop::<LinkEvent>(ev);
        }
        while let Some(ev) = board.recv() {
            drop::<LinkEvent>(ev);
        }
        if !moved {
            break;
        }
    }
    frames
}

fn nonce() -> u32 {
    let mut b = [0u8; 4];
    entropy(&mut b);
    u32::from_le_bytes(b)
}
