//! A [`SplitChip`] on the host, for the shared code's tests: a 4 MB NOR
//! array (per test thread), a C6-shaped layout 1 on 32 KiB pages, and an
//! MMU entry 0 the test sets.

extern crate std;

use alloc::vec;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use lp_bootctl::{ResetKind, SplitLayout};
use lpc_update::BoardState;

use super::split_chip::{FeedWatchdog, ScratchWindow, Sha256Stream, SplitChip, UpdateLight};
use super::split_statics::SplitStatics;

pub const FLASH_LEN: usize = 0x40_0000;
const PAGE: u32 = 0x8000;

std::thread_local! {
    static FLASH: RefCell<Vec<u8>> = RefCell::new(vec![0xff; FLASH_LEN]);
    static ENTRY0: Cell<u32> = const { Cell::new(0) };
    static RESET: Cell<ResetKind> = const { Cell::new(ResetKind::Warm) };
    static UPDATES_OFF: Cell<Option<&'static str>> = const { Cell::new(None) };
}

static SPLIT: SplitStatics<TestChip> = SplitStatics::new();

pub struct TestChip;

impl TestChip {
    /// A blank part, the core at `core_off` behind MMU entry 0, this reset.
    pub fn reset(core_off: u32, reset: ResetKind) {
        FLASH.with(|f| f.borrow_mut().fill(0xff));
        ENTRY0.with(|e| e.set(core_off));
        RESET.with(|r| r.set(reset));
        UPDATES_OFF.with(|o| o.set(None));
    }

    /// The part takes no update, for `why` (the S3's octal flash).
    pub fn updates_off(why: &'static str) {
        UPDATES_OFF.with(|o| o.set(Some(why)));
    }

    /// Put `bytes` at `at`, as a flasher would (no fence, no NOR rule).
    pub fn flash(at: u32, bytes: &[u8]) {
        FLASH.with(|f| {
            f.borrow_mut()[at as usize..at as usize + bytes.len()].copy_from_slice(bytes)
        });
    }

    /// The part's bytes at `at`.
    pub fn bytes(at: u32, len: usize) -> Vec<u8> {
        FLASH.with(|f| f.borrow()[at as usize..at as usize + len].to_vec())
    }
}

impl SplitChip for TestChip {
    const LAYOUT_CODE: u16 = 1;
    const REGION_START: u32 = lp_bootctl::REGION_START;
    const FALLBACK_REGION_END: u32 = FLASH_LEN as u32;
    const ENGINE_DATA_VADDR: usize = 0;
    const CORES_MUST_READ_TREE_STORE: bool = true;

    fn updates_off() -> Option<&'static str> {
        UPDATES_OFF.with(Cell::get)
    }

    fn page_size() -> u32 {
        PAGE
    }

    fn layout(offset: u32, len: u32) -> Option<SplitLayout> {
        SplitLayout::from_factory(offset, len, PAGE)
    }

    fn map_engine(_start: u32, _len: u32) {}

    fn running_core_offset() -> u32 {
        ENTRY0.with(Cell::get)
    }

    type Scratch = TestScratch;

    unsafe fn flash_read(at: u32, dst: *mut u32, len: u32) -> bool {
        let bytes = Self::bytes(at, len as usize);
        // SAFETY: the caller's buffer holds `len` bytes.
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), dst.cast(), len as usize) };
        true
    }

    unsafe fn flash_program(at: u32, src: *const u32, len: u32) -> bool {
        // SAFETY: the caller's buffer holds `len` bytes.
        let src = unsafe { core::slice::from_raw_parts(src.cast::<u8>(), len as usize) };
        FLASH.with(|f| {
            let mut f = f.borrow_mut();
            for (cell, b) in f[at as usize..at as usize + src.len()].iter_mut().zip(src) {
                *cell &= *b; // NOR: a program only clears bits
            }
        });
        true
    }

    unsafe fn flash_erase_sector(index: u32) -> bool {
        erase(index * 0x1000, 0x1000)
    }

    unsafe fn flash_erase_block(index: u32) -> bool {
        erase(index * 0x1_0000, 0x1_0000)
    }

    unsafe fn flash_unlock() -> bool {
        true
    }

    type Sha = TestSha;

    fn reset_kind() -> ResetKind {
        RESET.with(Cell::get)
    }

    fn reset_now() -> ! {
        panic!("the test chip reset");
    }

    type Watchdog = ();
    type Light = ();

    fn heap_free() -> usize {
        0
    }

    fn log_stack_if_grown(_what: &'static str) {}

    fn first_light_decided() -> bool {
        true
    }

    fn statics() -> &'static SplitStatics<Self> {
        &SPLIT
    }
}

fn erase(at: u32, len: usize) -> bool {
    FLASH.with(|f| f.borrow_mut()[at as usize..at as usize + len].fill(0xff));
    true
}

/// A copy of the bytes, standing in for a cache window.
pub struct TestScratch(Vec<u8>);

impl ScratchWindow for TestScratch {
    fn map(start: u32, len: u32) -> Option<Self> {
        Some(Self(TestChip::bytes(start, len as usize)))
    }

    fn bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Not a SHA-256: the shared code's tests here never hash.
pub struct TestSha;

impl Sha256Stream for TestSha {
    fn new() -> Self {
        Self
    }

    fn update(&mut self, _data: &[u8]) {}

    fn finalize(self) -> [u8; 32] {
        [0; 32]
    }
}

impl FeedWatchdog for () {
    fn feed(&mut self, _now_ms: u64) {}
}

impl UpdateLight for () {
    fn show(&mut self, _state: BoardState) {}
}
