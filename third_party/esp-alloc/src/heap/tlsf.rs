use core::{alloc::Layout, ptr::NonNull};

use rlsf::Tlsf;

// TODO: make this configurable
//
// LP fork, RAM research E6 (throwaway): `LP_E06_TLSF_FLLEN=<n>` at build time
// shrinks the first-level count. The control block is `FLLEN × SLLEN` list
// heads (4 KiB per region at 32 × 32 on rv32, ×`MAX_REGIONS` slots in
// `.bss`); a pool below `2^(4 + FLLEN)` bytes places every block exactly as
// at 32, since no size maps to a first level ≥ `FLLEN`. Unset, it is 32.
type Heap = Tlsf<'static, usize, usize, E06_FLLEN, { usize::BITS as usize }>;

const E06_FLLEN: usize = parse_fllen(option_env!("LP_E06_TLSF_FLLEN"));

const fn parse_fllen(text: Option<&str>) -> usize {
    let Some(text) = text else {
        return usize::BITS as usize;
    };
    let bytes = text.as_bytes();
    let mut value = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        assert!(bytes[i].is_ascii_digit(), "LP_E06_TLSF_FLLEN must be decimal");
        value = value * 10 + (bytes[i] - b'0') as usize;
        i += 1;
    }
    value
}

pub(crate) struct TlsfHeap {
    heap: Heap,
    pool_start: usize,
    pool_end: usize,
}

impl TlsfHeap {
    pub unsafe fn new(heap_bottom: *mut u8, size: usize) -> Self {
        let mut heap = Heap::new();

        let block = unsafe { core::slice::from_raw_parts(heap_bottom, size) };
        let actual_size = unsafe { heap.insert_free_block_ptr(block.into()).unwrap() };

        Self {
            heap,
            pool_start: heap_bottom as usize,
            pool_end: heap_bottom as usize + actual_size.get(),
        }
    }

    pub fn size(&self) -> usize {
        self.pool_end - self.pool_start
    }

    pub fn used(&self) -> usize {
        let mut used = 0;
        let pool =
            unsafe { core::slice::from_raw_parts(self.pool_start as *const u8, self.size()) };
        for block in unsafe { self.heap.iter_blocks(NonNull::from(pool)) } {
            if block.is_occupied() {
                used += block.size();
            }
        }
        used
    }

    pub fn free(&self) -> usize {
        let mut free = 0;
        let pool =
            unsafe { core::slice::from_raw_parts(self.pool_start as *const u8, self.size()) };
        for block in unsafe { self.heap.iter_blocks(NonNull::from(pool)) } {
            if !block.is_occupied() {
                free += block.max_payload_size();
            }
        }
        free
    }

    pub fn allocate(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        self.heap.allocate(layout)
    }

    pub(crate) unsafe fn try_deallocate(&mut self, ptr: NonNull<u8>, layout: Layout) -> bool {
        let addr = ptr.addr().get();
        if self.pool_start <= addr && self.pool_end > addr {
            unsafe { self.heap.deallocate(ptr, layout.align()) };
            true
        } else {
            false
        }
    }
}
