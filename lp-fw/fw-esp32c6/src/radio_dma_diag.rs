//! Diagnostic: where the radio's DMA buffers are (`radio_dma_diag`,
//! **never shipped**, CI never builds it).
//!
//! The question it answers for the Wi-Fi desk walk (plan P08, step W10):
//! does anything a radio DMA engine can write live in the second-stage
//! bootloader's load range (`0x4086B910..0x4087E610`: main RAM's last
//! 11,520 B and all of `dram2_seg`), with
//! the station joined and traffic flowing? The bootloader loads there after
//! an HP-only warm reset while the radio keeps running, and a stray DMA
//! write crashed it on 11 of 22 resets before #985
//! (`docs/defects/2026-10-05-a-requested-reboot-crashed-the-c6-bootloader.md`).
//! With #985 every radio C block goes to `HEAP_RADIO` first and the main
//! region when that is full, never to the reclaimed tail (and since RAM
//! research E4 `dram2_seg` is the main stack); this checks it on the
//! board under Wi-Fi's load.
//!
//! - **The radio's C blocks.** [`crate::c_heap`] records every live block
//!   the blobs hold (address and size, the blobs' DMA descriptors and
//!   buffers among them) in a fixed table, and [`log`] prints them grouped
//!   by region, flagging any inside the bootloader's load range.
//! - **esp-radio's Rust-side allocations.** The Rust heap's map
//!   (`heap_map_diag`, which this feature turns on) is printed beside it,
//!   so a live span in that range after joining and after an upload can be
//!   read off; esp-radio's Rust side holds packet *pointers* to blob
//!   buffers, so a Rust span there is not by itself a DMA target, but it is
//!   listed for the walk to judge.
//!
//! [`log`] runs from the heartbeat while the station is joined: the full
//! list once, then one summary line per heartbeat.

use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};

use critical_section::Mutex;

/// The bootloader's load range: from the lowest address a bootloader we ship
/// loads into (`build.rs`'s `BOOTLOADER_LOW`) to the top of `dram2_seg`.
pub const BOOTLOADER_RANGE: core::ops::Range<usize> = 0x4086_B910..0x4087_E610;

/// Live C blocks the table holds; a block past it is counted, not listed.
const TABLE: usize = 256;

struct Blocks {
    live: [(usize, usize); TABLE],
    len: usize,
    untracked: usize,
}

static BLOCKS: Mutex<RefCell<Blocks>> = Mutex::new(RefCell::new(Blocks {
    live: [(0, 0); TABLE],
    len: 0,
    untracked: 0,
}));
static LISTED: AtomicBool = AtomicBool::new(false);

/// A C block was allocated at `addr` (its header), `size` bytes.
pub fn record(addr: usize, size: usize) {
    critical_section::with(|cs| {
        let mut blocks = BLOCKS.borrow_ref_mut(cs);
        if blocks.len < TABLE {
            let at = blocks.len;
            blocks.live[at] = (addr, size);
            blocks.len += 1;
        } else {
            blocks.untracked += 1;
        }
    });
}

/// The C block at `addr` was freed.
pub fn forget(addr: usize) {
    critical_section::with(|cs| {
        let mut blocks = BLOCKS.borrow_ref_mut(cs);
        let len = blocks.len;
        if let Some(at) = blocks.live[..len].iter().position(|(a, _)| *a == addr) {
            blocks.live[at] = blocks.live[len - 1];
            blocks.len -= 1;
        } else if blocks.untracked > 0 {
            blocks.untracked -= 1;
        }
    });
}

/// Print where the radio's live C blocks are (every block the first time,
/// a summary after), and the Rust heap's map. `tag` names the moment.
pub fn log(tag: &str) {
    let snapshot = critical_section::with(|cs| {
        let blocks = BLOCKS.borrow_ref(cs);
        (blocks.live, blocks.len, blocks.untracked)
    });
    let (live, len, untracked) = snapshot;
    let (radio_start, radio_size) = crate::board::esp32c6::init::radio_region();
    let radio = radio_start..radio_start + radio_size;
    let mut in_radio = (0usize, 0usize);
    let mut in_boot = (0usize, 0usize);
    let mut elsewhere = (0usize, 0usize);
    for &(addr, size) in &live[..len] {
        let bucket = if BOOTLOADER_RANGE.contains(&addr) {
            &mut in_boot
        } else if radio.contains(&addr) {
            &mut in_radio
        } else {
            &mut elsewhere
        };
        bucket.0 += 1;
        bucket.1 += size;
    }
    log::info!(
        "[radio-dma] {tag}: {len} live C blocks ({untracked} untracked) · HEAP_RADIO {} ({} B) · \
         main {} ({} B) · bootloader range {} ({} B){}",
        in_radio.0,
        in_radio.1,
        elsewhere.0,
        elsewhere.1,
        in_boot.0,
        in_boot.1,
        if in_boot.0 == 0 {
            ""
        } else {
            " — IN THE BOOTLOADER'S REGION"
        }
    );
    let first = !LISTED.swap(true, Ordering::Relaxed);
    for &(addr, size) in &live[..len] {
        let flagged = BOOTLOADER_RANGE.contains(&addr);
        if first || flagged {
            log::info!(
                "[radio-dma]   {addr:#010x}..{:#010x} {size} B{}",
                addr + size,
                if flagged { " BOOTLOADER_RANGE" } else { "" }
            );
        }
    }
    if first {
        crate::heap_map::log(tag);
    }
}
