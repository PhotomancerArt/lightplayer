//! The guest arena: one contiguous, never-moving allocation the whole guest
//! address space is mapped flat into, with a real unmapped guard after it
//! wherever the host can provide one.
//!
//! # Why this is not a `Vec<u8>`
//!
//! M7 JD4 gives the arena to the bus, and a translated module has to address
//! *those* bytes rather than a copy — copying ~256 MiB in and out around every
//! slice would cost more than translation saves. So a native wasmtime host
//! hands the arena to the engine through `wasmtime::MemoryCreator`, and that
//! trait's contract is explicit:
//!
//! > JIT code will elide bounds checks based on the `guard_size_in_bytes`
//! > provided, so for JIT code to work correctly the memory returned will need
//! > to be properly guarded with `guard_size_in_bytes` bytes left unmapped
//! > after the base allocation.
//!
//! A `Vec` is a malloc'd block; what follows it is other heap objects, not
//! unmapped space. Promising a guard and supplying a `Vec` would turn a
//! translator bug from a wasm trap into a silent write into the emulator's own
//! data structures. The alternative — asking the engine for explicit bounds
//! checks — is what the M7 spike measured at 308.7 against 1,481 M
//! instructions/second on the same module bytes.
//!
//! So the arena maps its own address space: [`RESERVATION`] bytes of
//! reservation with the live bytes readable and writable at the front and
//! everything past them `PROT_NONE`, followed by [`GUARD`] more bytes of
//! `PROT_NONE`. The bus still owns it, JD4 is intact, and the guard is real.
//!
//! # Where there is no `mmap`
//!
//! The emulator also builds for `wasm32-wasip1` — the browser rig — and there
//! is no `mmap` there. It is also not needed there: in the browser the
//! emulator *is* a wasm module, the arena is bytes at an offset inside its own
//! linear memory, and the engine's own guard pages are already under it.
//!
//! On any such host the arena falls back to a plain heap allocation and
//! [`GuestArena::guard`] reports [`None`]. A host must read that rather than
//! assume: an arena that cannot say it is guarded cannot be configured as if
//! it were, which is the one bug this module exists to make unspellable.

use alloc::vec::Vec;
use core::ops::{Deref, DerefMut};

/// The address-space reservation a guarded arena makes, and the value a
/// wasmtime host must configure as `memory_reservation` (M7 JD18).
///
/// 4 GiB, because that is the threshold at which cranelift will elide the
/// bounds check on a 32-bit linear memory: every address a wasm module can
/// compute is `base + zext(u32 index) + static offset`, so a 4 GiB
/// reservation plus a guard covers all of them.
pub const RESERVATION: usize = 1 << 32;

/// The unmapped guard a guarded arena keeps after [`RESERVATION`], and the
/// value a wasmtime host must configure as `memory_guard_size` (M7 JD18).
///
/// 2 GiB, which covers any static offset a 32-bit module can encode.
pub const GUARD: usize = 1 << 31;

/// What a guarded arena promises the bytes after its base look like.
///
/// Only ever produced by [`GuestArena::guard`], and only for an arena that
/// really did map its own address space — so a host cannot name a guard it
/// does not have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaGuard {
    /// Bytes of reservation from the arena's base. Everything in it past the
    /// arena's live length is unmapped.
    pub reservation: usize,
    /// Unmapped bytes after `reservation`.
    pub guard: usize,
}

/// How the bytes are owned.
enum Owned {
    /// A plain heap allocation. No guard, and [`GuestArena::guard`] says so.
    Heap(Vec<u8>),
    /// An `mmap`ed reservation of `span` bytes at `base`. Never constructed
    /// on a host where [`map`] cannot succeed.
    Mapped { base: *mut u8, span: usize },
}

/// One contiguous guest arena.
///
/// Derefs to its bytes, so it reads exactly like the `Vec<u8>` it replaces.
pub struct GuestArena {
    /// How many readable, writable bytes the bus asked for. A mapping rounds
    /// its writable prefix up to a host page; those extra bytes are not part
    /// of the arena.
    len: usize,
    /// `Some` only for a mapped arena.
    guard: Option<ArenaGuard>,
    owned: Owned,
}

// SAFETY: `GuestArena` owns its allocation exclusively — `base` is derived
// from `owned` and no copy of it escapes except as a borrow of `self` — so
// moving one between threads is exactly as sound as moving the `Vec<u8>` it
// replaces.
unsafe impl Send for GuestArena {}
// SAFETY: a shared borrow hands out `&[u8]` and nothing else, so two threads
// holding `&GuestArena` can only read the same bytes, as with `Vec<u8>`.
unsafe impl Sync for GuestArena {}

impl GuestArena {
    /// An arena with no bytes yet. Unguarded, because there is nothing to
    /// guard.
    #[must_use]
    pub fn empty() -> Self {
        Self::from_heap(Vec::new())
    }

    /// `len` zeroed bytes, guarded if this host can guard them.
    ///
    /// Falls back to a heap allocation — reporting no guard — when the host
    /// has no `mmap`, when `len` does not fit a 32-bit linear memory, or when
    /// the reservation cannot be made. A fallback is slower to run translated
    /// guest code against, never wrong.
    #[must_use]
    pub fn zeroed(len: usize) -> Self {
        match map(len) {
            Some((base, span, guard)) => Self {
                len,
                guard: Some(guard),
                owned: Owned::Mapped { base, span },
            },
            None => Self::from_heap(alloc::vec![0u8; len]),
        }
    }

    fn from_heap(heap: Vec<u8>) -> Self {
        Self {
            len: heap.len(),
            guard: None,
            owned: Owned::Heap(heap),
        }
    }

    /// What the bytes after the base look like, or `None` when this arena is
    /// a plain heap allocation and nothing may promise a guard for it.
    #[must_use]
    pub fn guard(&self) -> Option<ArenaGuard> {
        self.guard
    }

    /// The base pointer. Stable for the life of this arena: a mapping never
    /// moves, and the heap fallback's `Vec` is never resized.
    #[must_use]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.deref_mut().as_mut_ptr()
    }
}

impl Default for GuestArena {
    fn default() -> Self {
        Self::empty()
    }
}

impl Deref for GuestArena {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match &self.owned {
            Owned::Heap(heap) => heap,
            // SAFETY: `base` addresses the read/write prefix of a live
            // mapping the kernel zero-filled, and `map` made that prefix at
            // least `len` bytes long (it rounds `len` up to a host page).
            // `&self` borrows the arena for the slice's lifetime, and the
            // mapping can neither move nor be released while it is borrowed.
            Owned::Mapped { base, .. } => unsafe {
                core::slice::from_raw_parts(*base, self.len)
            },
        }
    }
}

impl DerefMut for GuestArena {
    fn deref_mut(&mut self) -> &mut [u8] {
        let len = self.len;
        match &mut self.owned {
            Owned::Heap(heap) => heap,
            // SAFETY: as for `deref`, and `&mut self` is exclusive, so no
            // other borrow of these bytes exists.
            Owned::Mapped { base, .. } => unsafe {
                core::slice::from_raw_parts_mut(*base, len)
            },
        }
    }
}

impl Drop for GuestArena {
    fn drop(&mut self) {
        if let Owned::Mapped { base, span } = self.owned {
            unmap(base, span);
        }
    }
}

/// Reserve `RESERVATION + GUARD` bytes of address space with `len` of them
/// readable and writable at the front, or `None` when this host cannot.
///
/// Returns `(base, span, guard)`.
#[cfg(all(unix, not(target_family = "wasm")))]
fn map(len: usize) -> Option<(*mut u8, usize, ArenaGuard)> {
    if len == 0 || len > RESERVATION {
        // Nothing to guard, or too large to be a 32-bit linear memory at all.
        return None;
    }
    // SAFETY: `sysconf` reads a process-global constant and touches no memory
    // of ours.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let page = usize::try_from(page).ok().filter(|p| p.is_power_of_two())?;
    // `mprotect` works in whole pages, so the writable prefix rounds up. The
    // rounding slop sits past `len`, and the arena is `len` bytes long.
    let rw = len.checked_next_multiple_of(page)?;
    let span = RESERVATION.checked_add(GUARD)?;

    // SAFETY: an anonymous `MAP_PRIVATE` mapping at a kernel-chosen address.
    // The length is non-zero, no file descriptor is involved (`-1`, as
    // `MAP_ANON` requires), and `PROT_NONE` means nothing in it is readable
    // until the `mprotect` below. Passing a null hint means the kernel cannot
    // replace a mapping we already hold.
    let base = unsafe {
        libc::mmap(
            core::ptr::null_mut(),
            span,
            libc::PROT_NONE,
            libc::MAP_PRIVATE | libc::MAP_ANON,
            -1,
            0,
        )
    };
    if base == libc::MAP_FAILED {
        return None;
    }

    // SAFETY: `base` is a live mapping of `span` bytes that we own, `rw <=
    // RESERVATION <= span` is page-aligned, and no pointer into the mapping
    // has escaped yet.
    let ok = unsafe { libc::mprotect(base, rw, libc::PROT_READ | libc::PROT_WRITE) } == 0;
    if !ok {
        // SAFETY: `base`/`span` are exactly what `mmap` just returned, and no
        // pointer into the mapping has escaped.
        unsafe { libc::munmap(base, span) };
        return None;
    }

    Some((
        base.cast::<u8>(),
        span,
        ArenaGuard {
            reservation: RESERVATION,
            guard: GUARD,
        },
    ))
}

/// The hosts with no `mmap` — `wasm32-wasip1` above all, where the engine
/// already owns a guard-paged linear memory and the arena is bytes inside it.
#[cfg(not(all(unix, not(target_family = "wasm"))))]
fn map(_len: usize) -> Option<(*mut u8, usize, ArenaGuard)> {
    None
}

#[cfg(all(unix, not(target_family = "wasm")))]
fn unmap(base: *mut u8, span: usize) {
    // SAFETY: `base` and `span` are the exact pair `map` returned for this
    // arena; `Drop` runs once, and every borrow of the bytes ended with the
    // borrow of the arena that produced it.
    unsafe { libc::munmap(base.cast::<core::ffi::c_void>(), span) };
}

/// Unreachable in practice: [`map`] never succeeds on such a host, so no
/// arena is ever `Owned::Mapped` and nothing is ever released here.
#[cfg(not(all(unix, not(target_family = "wasm"))))]
fn unmap(_base: *mut u8, _span: usize) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zeroed_arena_reads_as_zero_and_writes_back() {
        let mut arena = GuestArena::zeroed(64 * 1024);
        assert_eq!(arena.len(), 64 * 1024);
        assert!(arena.iter().all(|&b| b == 0));
        arena[1234] = 0xab;
        assert_eq!(arena[1234], 0xab);
        assert_eq!(arena[1233], 0);
    }

    #[test]
    fn an_empty_arena_is_unguarded() {
        let arena = GuestArena::empty();
        assert!(arena.is_empty());
        assert_eq!(arena.guard(), None);
    }

    #[cfg(all(unix, not(target_family = "wasm")))]
    #[test]
    fn a_mapped_arena_reports_the_reservation_it_made() {
        let arena = GuestArena::zeroed(1 << 20);
        assert_eq!(
            arena.guard(),
            Some(ArenaGuard {
                reservation: RESERVATION,
                guard: GUARD,
            })
        );
    }

    #[test]
    fn an_arena_too_large_for_a_32_bit_memory_is_never_mapped() {
        // `map` refuses on the length before it reserves anything, so
        // `zeroed` would fall back to the heap and report no guard.
        assert!(map(RESERVATION + 1).is_none());
    }
}
