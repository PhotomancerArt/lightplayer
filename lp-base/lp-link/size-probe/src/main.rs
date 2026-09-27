//! Links one lp-link variant into a bare riscv32 image, driving every entry
//! point with inputs the optimizer cannot see (volatile reads), so what is
//! linked is what a firmware using the link would carry. Feature `base`
//! links none of it; the difference is the link's cost.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};
use core::fmt::{self, Write};
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

static mut INPUT: [u8; 512] = [0; 512];
static mut SINK: u8 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // The baseline every firmware image already has: allocation, a VecDeque,
    // memmove, 64-bit division, and integer / string formatting.
    let mut v: Vec<u8> = Vec::new();
    v.push(input(0));
    v.extend_from_slice(&[input(1); 8]);
    v.copy_within(1..5, input(2) as usize);
    let mut q: VecDeque<u8> = VecDeque::new();
    q.push_back(input(3));
    let wide = (input(4) as u64) << 40;
    let _ = write!(
        Sink,
        "{} {} {} {} {}",
        v[0],
        q.pop_front().unwrap_or(0) as u32,
        wide / (input(5) as u64 + 1),
        wide % (input(6) as u64 + 1),
        "s"
    );

    #[cfg(feature = "noarq")]
    exercise::<lp_link::NoArq>();
    #[cfg(feature = "sw")]
    exercise::<lp_link::StopAndWait>();
    #[cfg(feature = "gbn")]
    exercise::<lp_link::GoBackN<127>>();
    #[cfg(feature = "sr")]
    exercise::<lp_link::SelectiveRepeat>();
    loop {}
}

/// The two checksums as standalone symbols, for reading their inner loops
/// (`rust-objdump -d --disassemble-symbols=probe_crc32c`).
#[cfg(feature = "crcfns")]
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn probe_crc32c(p: *const u8, n: usize) -> u32 {
    // SAFETY: the caller passes a valid buffer.
    lp_link::crc::crc32c(0, unsafe { core::slice::from_raw_parts(p, n) })
}

#[cfg(feature = "crcfns")]
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn probe_crc16(p: *const u8, n: usize) -> u16 {
    // SAFETY: the caller passes a valid buffer.
    lp_link::crc::crc16_ccitt(0xFFFF, unsafe { core::slice::from_raw_parts(p, n) })
}

#[cfg(feature = "crcfns")]
#[used]
static KEEP_CRC: [extern "C" fn(*const u8, usize) -> u32; 1] = [probe_crc32c];
#[cfg(feature = "crcfns")]
#[used]
static KEEP_CRC16: [extern "C" fn(*const u8, usize) -> u16; 1] = [probe_crc16];

/// `size_of` a link on this target, readable as a symbol's size (`nm -S`).
#[cfg(feature = "sr")]
#[used]
#[unsafe(no_mangle)]
static LINK_STRUCT_SIZE_SR: [u8; core::mem::size_of::<lp_link::Link<lp_link::SelectiveRepeat>>()] =
    [0; core::mem::size_of::<lp_link::Link<lp_link::SelectiveRepeat>>()];

#[cfg(feature = "gbn")]
#[used]
#[unsafe(no_mangle)]
static LINK_STRUCT_SIZE_GBN: [u8; core::mem::size_of::<lp_link::Link<lp_link::GoBackN<127>>>()] =
    [0; core::mem::size_of::<lp_link::Link<lp_link::GoBackN<127>>>()];

#[cfg(not(feature = "base"))]
static mut LOG_RING: lp_link::LogRing<1024> = lp_link::LogRing::new();

#[cfg(not(feature = "base"))]
fn exercise<A: lp_link::Arq>() {
    use lp_link::{CH_LOG, CrcKind, Framing, LinkConfig, LinkEvent};
    let mut cfg = LinkConfig::usb();
    cfg.max_payload = u16::from_le_bytes([input(1), input(2)]);
    cfg.tx_window = input(3);
    cfg.rx_window = input(4);
    if input(5) != 0 {
        cfg.framing = Framing::Datagram;
    }
    cfg.crc = if cfg!(feature = "crc16") {
        CrcKind::Crc16
    } else {
        CrcKind::Crc32c
    };
    let mut link = lp_link::Link::<A>::new(
        cfg,
        u32::from_le_bytes([input(6), input(7), input(8), input(9)]),
    );
    // SAFETY: single-threaded bare-metal probe; nothing else touches it.
    let ring = unsafe { &mut *addr_of_mut!(LOG_RING) };
    let mut now = 0u64;
    loop {
        now += input(10) as u64;
        let n = input(11) as usize;
        // SAFETY: as above.
        let bytes = unsafe { &(&*addr_of!(INPUT))[..n] };
        match input(12) {
            0 => link.on_bytes(now, bytes),
            1 => link.on_datagram(now, bytes),
            2 => {
                let _ = link.send(input(13), bytes);
            }
            3 => lp_link::link_log!(ring, input(14), "fps {} heap {}", n, now),
            4 => link.restart(now),
            _ => {}
        }
        link.pump_log(now, ring, CH_LOG);
        while let Some(f) = link.poll_transmit(now) {
            sink(f);
        }
        while let Some(ev) = link.recv() {
            if let LinkEvent::Message { data, .. } | LinkEvent::Text(data) = ev {
                sink(&data);
            }
        }
        let c = link.counters();
        sink(&[
            c.retransmits as u8,
            c.bad_frames as u8,
            link.poll_timeout().unwrap_or(0) as u8,
        ]);
    }
}

fn input(i: usize) -> u8 {
    // SAFETY: a plain read of a static the optimizer must treat as unknown.
    unsafe { read_volatile(addr_of!(INPUT[i])) }
}

fn sink(bytes: &[u8]) {
    for &b in bytes {
        // SAFETY: write-only volatile sink.
        unsafe { write_volatile(addr_of_mut!(SINK), b) }
    }
}

struct Sink;

impl Write for Sink {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        sink(s.as_bytes());
        Ok(())
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    let _ = write!(Sink, "{info}");
    loop {}
}

/// A bump allocator over a static arena: identical in every build, so it
/// cancels out of the deltas.
struct Bump;

static mut ARENA: [u8; 65536] = [0; 65536];
static mut NEXT: usize = 0;

// SAFETY: single-threaded; never frees; alignment honoured.
unsafe impl GlobalAlloc for Bump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            let start = (NEXT + layout.align() - 1) & !(layout.align() - 1);
            if start + layout.size() > 65536 {
                return core::ptr::null_mut();
            }
            NEXT = start + layout.size();
            addr_of_mut!(ARENA).cast::<u8>().add(start)
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOC: Bump = Bump;
