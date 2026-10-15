//! lp-tree-store's RV32 size probe: every public entry point the firmware
//! will call (format, mount, put, append, put_chunk_deflated, get,
//! file_size, exists, list, delete, delete_prefix, begin/commit/abort)
//! on a RAM flash. No `stats`: the firmware builds without that feature.
//!
//! The hasher is the firmware's to inject (the C6 has SHA in hardware), so
//! by default the probe passes a stand-in that costs a few bytes: the ELF
//! is the store itself. `--features soft-sha` links `sha2` instead, for
//! the store + a software SHA-256. `--features lpfs` makes the same calls
//! through `LpFsTree` as a `dyn lpfs::LpFs` (every trait method linked, as
//! the server holds it), for the store plus its adapter.

#![no_std]
#![no_main]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::hint::black_box;
use core::panic::PanicInfo;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicUsize, Ordering};

#[cfg(not(feature = "soft-sha"))]
use lp_tree_store::ObjectHasher;
use lp_tree_store::{Flash, StoreConfig, StoreError, TreeStore};

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
        for (c, d) in cells
            .get_mut(a..a + data.len())
            .ok_or(())?
            .iter_mut()
            .zip(data)
        {
            *c &= *d;
        }
        Ok(())
    }
    fn erase_sector(&mut self, sector: u32) -> Result<(), ()> {
        let s = (sector * SECTOR) as usize;
        cells()
            .get_mut(s..s + SECTOR as usize)
            .ok_or(())?
            .fill(0xFF);
        Ok(())
    }
}

fn cells() -> &'static mut [u8] {
    // SAFETY: single-threaded probe; the only access path to CELLS.
    unsafe { &mut *addr_of_mut!(CELLS) }
}

/// The firmware's hardware SHA stands here: an opaque call the optimizer
/// cannot see through, costing about what a driver call does.
#[cfg(not(feature = "soft-sha"))]
struct ProbeHasher;

#[cfg(not(feature = "soft-sha"))]
impl ObjectHasher for ProbeHasher {
    #[inline(never)]
    fn sha256(&mut self, parts: &[&[u8]]) -> [u8; 32] {
        let mut out = [0u8; 32];
        for p in parts {
            for (i, b) in p.iter().enumerate() {
                out[i % 32] ^= *b;
            }
        }
        black_box(out)
    }
}

#[cfg(feature = "soft-sha")]
type ProbeHasher = lp_tree_store::SoftSha256;
#[cfg(feature = "soft-sha")]
#[allow(non_upper_case_globals)]
const ProbeHasher: ProbeHasher = lp_tree_store::SoftSha256;

type ProbeStore = TreeStore<RamFlash, ProbeHasher>;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // The firmware's boot: mount by value, and format by value what a
    // `NoStore` mount handed back, so both name one `TreeStore<RamFlash,
    // ProbeHasher>` (`rust-nm` shows each store symbol once).
    let cfg = StoreConfig::default();
    let st = match TreeStore::mount(RamFlash, ProbeHasher, black_box(cfg.clone())) {
        Ok(st) => Some(st),
        Err((StoreError::NoStore, flash, hasher)) => TreeStore::format(flash, hasher, cfg).ok(),
        Err(_) => None,
    };
    if let Some(st) = st {
        black_box(st.summary());
        calls(st);
    }
    loop {}
}

#[cfg(not(feature = "lpfs"))]
fn calls(mut st: ProbeStore) {
    {
        let _ = st.put("/projects/a/project.json", black_box(b"{}"));
        let _ = st.append("/projects/a/project.json", black_box(b"\n"));
        let _ = st.begin();
        let _ = st.put_chunk_deflated("/a/s.glsl", 0, 3, None, black_box(&[3, 0, 1, 2, 3]));
        let _ = st.delete_prefix(black_box("/old/"));
        let _ = black_box(st.commit());
        let _ = st.abort();
        let _ = black_box(st.delete(black_box("/x")));
        let _ = black_box(st.get(black_box("/projects/a/project.json")));
        let _ = black_box(st.file_size(black_box("/a")));
        let _ = black_box(st.exists(black_box("/b")));
        let _ = black_box(st.list(black_box("/")));
    }
}

#[cfg(feature = "lpfs")]
fn calls(st: ProbeStore) {
    use alloc::rc::Rc;
    use core::cell::RefCell;
    use lpfs::{FsVersion, LpFs, LpPath};
    let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(lp_tree_store::LpFsTree::new(st)));
    let p = |s: &'static str| LpPath::new(black_box(s));
    {
        let f = fs.borrow();
        let _ = f.write_file(p("/projects/a/project.json"), black_box(b"{}"));
        let _ = f.append_file(p("/projects/a/project.json"), black_box(b"\n"));
        let _ = f.begin_batch();
        let _ = f.delete_dir(p("/old"));
        let _ = black_box(f.commit_batch());
        let _ = f.abort_batch();
        let _ = black_box(f.delete_file(p("/x")));
        let _ = black_box(f.read_file(p("/projects/a/project.json")));
        let _ = black_box(f.file_size(p("/a")));
        let _ = black_box(f.file_exists(p("/b")));
        let _ = black_box(f.is_dir(p("/projects")));
        let _ = black_box(f.list_dir(p("/"), black_box(true)));
        let _ = black_box(f.chroot(p("/projects/a")));
        let _ = black_box(f.get_changes_since(FsVersion::default()));
    }
    fs.borrow_mut().clear_changes_before(FsVersion::default());
    black_box(fs);
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
