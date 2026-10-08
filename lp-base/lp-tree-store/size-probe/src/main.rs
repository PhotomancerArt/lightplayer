//! lp-tree-store's RV32 size probe: every public entry point a device needs
//! (format, mount, put, get, delete_prefix, list, commit) on a RAM flash.

#![no_std]
#![no_main]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::hint::black_box;
use core::panic::PanicInfo;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicUsize, Ordering};

use lp_tree_store::{Codec, Flash, StoreConfig, TreeStore};

const SECTORS: u32 = 16;
const SECTOR: u32 = 4096;
static mut CELLS: [u8; (SECTORS * SECTOR) as usize] = [0xFF; (SECTORS * SECTOR) as usize];

struct RamFlash;

impl Flash for RamFlash {
    type Error = ();
    fn sector_count(&self) -> u32 {
        SECTORS
    }
    fn sector_size(&self) -> u32 {
        SECTOR
    }
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), ()> {
        let cells = cells();
        let a = addr as usize;
        buf.copy_from_slice(cells.get(a..a + buf.len()).ok_or(())?);
        Ok(())
    }
    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), ()> {
        let cells = cells();
        let a = addr as usize;
        for (c, d) in cells.get_mut(a..a + data.len()).ok_or(())?.iter_mut().zip(data) {
            *c &= *d;
        }
        Ok(())
    }
    fn erase_sector(&mut self, sector: u32) -> Result<(), ()> {
        let s = (sector * SECTOR) as usize;
        cells().get_mut(s..s + SECTOR as usize).ok_or(())?.fill(0xFF);
        Ok(())
    }
}

fn cells() -> &'static mut [u8] {
    // SAFETY: single-threaded probe; the only access path to CELLS.
    unsafe { &mut *addr_of_mut!(CELLS) }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let cfg = StoreConfig {
        codec: Codec::Stored,
        ..StoreConfig::default()
    };
    let mut flash = RamFlash;
    let _ = black_box(TreeStore::format(&mut flash, &cfg));
    // Mount through `&mut` too, so format and mount share one
    // `TreeStore<&mut RamFlash>` instantiation (a by-value mount would link
    // the whole store twice).
    if let Ok(mut st) = TreeStore::mount(&mut flash, cfg) {
        let _ = st.put("/projects/a/project.json", black_box(b"{}"));
        let _ = st.delete_prefix(black_box("/old/"));
        let _ = black_box(st.commit());
        let _ = black_box(st.get(black_box("/projects/a/project.json")));
        let _ = black_box(st.list(black_box("/")));
        black_box(st.stats());
    }
    loop {}
}

// A bump allocator over a static heap: the probe never frees.
const HEAP_SIZE: usize = 192 * 1024;
static mut HEAP: [u8; HEAP_SIZE] = [0; HEAP_SIZE];
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Bump;

unsafe impl GlobalAlloc for Bump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let base = addr_of_mut!(HEAP) as usize;
        let mut cur = NEXT.load(Ordering::Relaxed);
        loop {
            let start = (base + cur).next_multiple_of(layout.align()) - base;
            let end = start + layout.size();
            if end > HEAP_SIZE {
                return core::ptr::null_mut();
            }
            match NEXT.compare_exchange(cur, end, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return (base + start) as *mut u8,
                Err(seen) => cur = seen,
            }
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOC: Bump = Bump;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}
