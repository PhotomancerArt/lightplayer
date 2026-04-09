## Phase 1: Add Shared Memory Region to lp-riscv-emu

Add a third memory region to `Memory` struct for shared memory at address 0x40000000.

### Code Organization

- Place new fields in `Memory` struct first
- Update access methods with three-way dispatch: code (0x0), shared (0x40000000), RAM (0x80000000)
- Keep constructors backward compatible (shared is optional)
- Place helper functions at the bottom

### Implementation Details

**File: `lp-riscv/lp-riscv-emu/src/emu/memory.rs`**

Add to `Memory` struct:
```rust
pub const DEFAULT_SHARED_START: u32 = 0x40000000;

pub struct Memory {
    code: Vec<u8>,
    shared: Option<SharedMemory>,  // NEW
    ram: Vec<u8>,
    code_start: u32,
    shared_start: u32,  // NEW
    ram_start: u32,
    allow_unaligned_access: bool,
}

/// Shared memory reference (external backing storage)
pub struct SharedMemory {
    data: core::cell::UnsafeCell<*mut u8>,  // or Arc<Mutex<Vec<u8>>> for std builds
    size: usize,
}
```

For `std` builds, use `Arc<parking_lot::Mutex<Vec<u8>>>` or similar for safe shared access between emulator instances.

Update constructor pattern:
```rust
pub fn new_with_shared(
    code: Vec<u8>,
    ram: Vec<u8>,
    shared: Arc<Mutex<Vec<u8>>>,  // or reference
    code_start: u32,
    shared_start: u32,
    ram_start: u32,
) -> Self
```

Keep existing `new()` and `with_default_addresses()` - they set `shared: None`.

Update `read_word_aligned` dispatch:
```rust
fn read_word_aligned(&self, address: u32) -> Result<i32, EmulatorError> {
    if address >= self.ram_start {
        // RAM region
    } else if address >= self.shared_start && self.shared.is_some() {
        // Shared region
    } else {
        // Code region
    }
}
```

Similar updates for:
- `write_word_aligned`
- `read_byte`
- `write_byte`
- `read_halfword_aligned`
- `write_halfword_aligned`
- `read_u8`
- `fetch_instruction` (may need to support code execution from shared? probably not)

Add helper methods:
```rust
pub fn shared(&self) -> Option<&SharedMemory>;
pub fn shared_start(&self) -> u32;
pub fn shared_end(&self) -> Option<u32>;
```

### Tests

Add unit tests in `lp-riscv/lp-riscv-emu/src/emu/memory.rs` (in `#[cfg(test)]` mod at top of file):

```rust
#[test]
fn shared_memory_read_write() {
    let shared = Arc::new(Mutex::new(vec![0u8; 1024]));
    let mem = Memory::new_with_shared(
        vec![],               // code
        vec![0; 1024],        // ram
        shared.clone(),       // shared
        0x0,                  // code_start
        0x40000000,           // shared_start
        0x80000000,           // ram_start
    );
    
    // Write to shared at offset 0
    mem.write_word(0x40000000, 42).unwrap();
    
    // Read back
    assert_eq!(mem.read_word(0x40000000).unwrap(), 42);
    
    // Verify underlying vec was modified
    let guard = shared.lock();
    assert_eq!(i32::from_le_bytes([guard[0], guard[1], guard[2], guard[3]]), 42);
}

#[test]
fn three_way_address_dispatch() {
    // Test that code (0x0), shared (0x40000000), and RAM (0x80000000) regions
    // are all accessible and distinct
}

#[test]
fn backward_compatible_without_shared() {
    // Existing Memory::new() should still work
    let mem = Memory::with_default_addresses(vec![], vec![0; 1024]);
    // shared memory access should return error
    assert!(mem.read_word(0x40000000).is_err());
}
```

### Validate

```bash
cargo test -p lp-riscv-emu
cargo check -p lp-riscv-emu --no-default-features  # no_std still works
```

Fix any warnings (unused imports, etc.) before proceeding.
