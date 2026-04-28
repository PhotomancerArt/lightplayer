//! Engine shared region: bump allocator with guest addresses in [`lp_riscv_emu::DEFAULT_SHARED_START`].

use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::sync::Arc;
use alloc::vec::Vec;

use lp_riscv_emu::DEFAULT_SHARED_START;
use lpvm::{AllocError, LpvmBuffer, LpvmMemory};

fn round_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    let mask = align - 1;
    match value.checked_add(mask) {
        Some(v) => v & !mask,
        None => usize::MAX,
    }
}

/// Default shared arena size (matches [`lpvm::BumpLpvmMemory`] default in spirit).
pub const DEFAULT_SHARED_CAPACITY: usize = 256 * 1024;

/// Bump allocator over the emulator shared-memory [`Vec`], exposed to the guest at [`DEFAULT_SHARED_START`].
#[derive(Clone)]
pub struct EmuSharedArena {
    storage: Arc<std::sync::Mutex<Vec<u8>>>,
    next: Arc<AtomicUsize>,
    shared_start: u32,
}

impl EmuSharedArena {
    /// New arena filled with zeros.
    pub fn new(capacity: usize) -> Self {
        Self {
            storage: Arc::new(std::sync::Mutex::new(vec![0u8; capacity])),
            next: Arc::new(AtomicUsize::new(0)),
            shared_start: DEFAULT_SHARED_START,
        }
    }

    /// Bump allocator over an existing backing store (same `Arc` as [`lp_riscv_emu::Memory::new_with_shared`]).
    /// `bump_start` is the first byte offset available for allocation (leave room for vmctx / headers).
    pub(crate) fn attach_shared_backing(
        storage: Arc<std::sync::Mutex<Vec<u8>>>,
        bump_start: usize,
    ) -> Self {
        Self {
            storage,
            next: Arc::new(AtomicUsize::new(bump_start)),
            shared_start: DEFAULT_SHARED_START,
        }
    }

    /// Same backing storage as [`LpvmMemory`] allocations (for [`lp_riscv_emu::Memory::new_with_shared`]).
    pub fn storage_arc(&self) -> Arc<std::sync::Mutex<Vec<u8>>> {
        self.storage.clone()
    }

    pub fn shared_start(&self) -> u32 {
        self.shared_start
    }

    /// Lock the shared backing buffer (guest addresses = [`Self::shared_start`] + index).
    pub fn lock_storage(&self) -> std::sync::MutexGuard<'_, Vec<u8>> {
        self.storage.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl LpvmMemory for EmuSharedArena {
    fn alloc(&self, size: usize, align: usize) -> Result<LpvmBuffer, AllocError> {
        if size == 0 {
            return Err(AllocError::InvalidSize);
        }
        if !align.is_power_of_two() {
            return Err(AllocError::InvalidSize);
        }
        let mut guard = self.storage.lock().map_err(|_| AllocError::OutOfMemory)?;
        let len = guard.len();
        loop {
            let pos = self.next.load(Ordering::Relaxed);
            let aligned = round_up(pos, align);
            let end = aligned.checked_add(size).ok_or(AllocError::InvalidSize)?;
            if end > len {
                return Err(AllocError::OutOfMemory);
            }
            match self
                .next
                .compare_exchange_weak(pos, end, Ordering::SeqCst, Ordering::Relaxed)
            {
                Ok(_) => {
                    let native = unsafe { guard.as_mut_ptr().add(aligned) };
                    let guest = self.shared_start as u64 + aligned as u64;
                    return Ok(LpvmBuffer::new(native, guest, size, align));
                }
                Err(_) => continue,
            }
        }
    }

    fn free(&self, _buffer: LpvmBuffer) {}

    fn realloc(&self, _buffer: LpvmBuffer, _new_size: usize) -> Result<LpvmBuffer, AllocError> {
        Err(AllocError::InvalidPointer)
    }
}
