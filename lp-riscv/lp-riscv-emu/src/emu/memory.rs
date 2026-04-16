//! Memory model for the RISC-V 32 emu.

use alloc::vec::Vec;

use super::error::{EmulatorError, MemoryAccessKind};

/// Default RAM start address (0x80000000, matching embive's RAM_OFFSET).
pub const DEFAULT_RAM_START: u32 = 0x80000000;

/// Default shared-memory start (LPVM guest heap / engine arena; between code and RAM).
pub const DEFAULT_SHARED_START: u32 = 0x4000_0000;

/// Memory model with separate code, optional shared, and RAM regions.
pub struct Memory {
    code: Vec<u8>,
    ram: Vec<u8>,
    code_start: u32,
    /// When `shared_backing` is `None`, must equal `ram_start` (empty shared range).
    shared_start: u32,
    ram_start: u32,
    #[cfg(feature = "std")]
    shared_backing: Option<std::sync::Arc<std::sync::Mutex<Vec<u8>>>>,
    /// When true, allow misaligned loads/stores (matches embedded targets like ESP32).
    allow_unaligned_access: bool,
}

impl Memory {
    /// Create a new memory model.
    ///
    /// # Arguments
    ///
    /// * `code` - Code region (read-only for stores)
    /// * `ram` - RAM region (read-write)
    /// * `code_start` - Base address for code (typically 0x0)
    /// * `ram_start` - Base address for RAM (typically 0x80000000)
    pub fn new(code: Vec<u8>, ram: Vec<u8>, code_start: u32, ram_start: u32) -> Self {
        Self {
            code,
            ram,
            code_start,
            shared_start: ram_start,
            ram_start,
            #[cfg(feature = "std")]
            shared_backing: None,
            allow_unaligned_access: false,
        }
    }

    /// Create memory with an external shared region (read/write, data only — not executable).
    ///
    /// Requires the `std` feature. `shared_start` must be strictly less than `ram_start`.
    /// The shared slice visible to the guest is `shared_start .. shared_start + vec.len()`.
    #[cfg(feature = "std")]
    pub fn new_with_shared(
        code: Vec<u8>,
        ram: Vec<u8>,
        shared_backing: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
        code_start: u32,
        shared_start: u32,
        ram_start: u32,
    ) -> Self {
        assert!(
            shared_start < ram_start,
            "shared_start must be below ram_start"
        );
        Self {
            code,
            ram,
            code_start,
            shared_start,
            ram_start,
            shared_backing: Some(shared_backing),
            allow_unaligned_access: false,
        }
    }

    /// Create a new memory model with default addresses (no shared region).
    pub fn with_default_addresses(code: Vec<u8>, ram: Vec<u8>) -> Self {
        Self::new(code, ram, 0x0, DEFAULT_RAM_START)
    }

    /// Enable misaligned memory access (matches embedded targets like ESP32).
    pub fn set_allow_unaligned_access(&mut self, allow: bool) {
        self.allow_unaligned_access = allow;
    }

    #[inline]
    fn has_shared(&self) -> bool {
        #[cfg(feature = "std")]
        {
            self.shared_backing.is_some()
        }
        #[cfg(not(feature = "std"))]
        {
            false
        }
    }

    #[inline]
    fn in_shared(&self, address: u32) -> bool {
        self.has_shared() && address >= self.shared_start && address < self.ram_start
    }

    #[inline]
    fn in_ram(&self, address: u32) -> bool {
        address >= self.ram_start
    }

    #[cfg(feature = "std")]
    fn shared_slice_len(&self) -> usize {
        self.shared_backing
            .as_ref()
            .map(|a| a.lock().unwrap().len())
            .unwrap_or(0)
    }

    /// Read a 32-bit word from memory.
    ///
    /// When allow_unaligned_access is enabled, supports unaligned addresses to match
    /// embedded targets like ESP32. Otherwise returns UnalignedAccess for misaligned addresses.
    pub fn read_word(&self, address: u32) -> Result<i32, EmulatorError> {
        if address % 4 != 0 && !self.allow_unaligned_access {
            return Err(EmulatorError::UnalignedAccess {
                address,
                alignment: 4,
                pc: 0,
                regs: [0; 32],
            });
        }
        if address % 4 == 0 {
            return self.read_word_aligned(address);
        }
        let b0 = self.read_u8(address)?;
        let b1 = self.read_u8(address.wrapping_add(1))?;
        let b2 = self.read_u8(address.wrapping_add(2))?;
        let b3 = self.read_u8(address.wrapping_add(3))?;
        Ok(i32::from_le_bytes([b0, b1, b2, b3]))
    }

    /// Read a 32-bit word from memory (aligned addresses only).
    fn read_word_aligned(&self, address: u32) -> Result<i32, EmulatorError> {
        if self.in_ram(address) {
            let offset = (address - self.ram_start) as usize;
            if offset + 4 > self.ram.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 4,
                    kind: MemoryAccessKind::Read,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            let bytes = [
                self.ram[offset],
                self.ram[offset + 1],
                self.ram[offset + 2],
                self.ram[offset + 3],
            ];
            return Ok(i32::from_le_bytes(bytes));
        }
        #[cfg(feature = "std")]
        if self.in_shared(address) {
            let arc = self.shared_backing.as_ref().unwrap();
            let v = arc.lock().unwrap();
            let offset = (address - self.shared_start) as usize;
            if offset + 4 > v.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 4,
                    kind: MemoryAccessKind::Read,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            let bytes = [v[offset], v[offset + 1], v[offset + 2], v[offset + 3]];
            return Ok(i32::from_le_bytes(bytes));
        }
        // Code region
        let offset = (address - self.code_start) as usize;
        if offset + 4 > self.code.len() {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 4,
                kind: MemoryAccessKind::Read,
                pc: 0,
                regs: [0; 32],
            });
        }
        let bytes = [
            self.code[offset],
            self.code[offset + 1],
            self.code[offset + 2],
            self.code[offset + 3],
        ];
        Ok(i32::from_le_bytes(bytes))
    }

    /// Write a 32-bit word to memory.
    ///
    /// When allow_unaligned_access is enabled, supports unaligned addresses.
    pub fn write_word(&mut self, address: u32, value: i32) -> Result<(), EmulatorError> {
        if address % 4 != 0 && !self.allow_unaligned_access {
            return Err(EmulatorError::UnalignedAccess {
                address,
                alignment: 4,
                pc: 0,
                regs: [0; 32],
            });
        }
        if address % 4 == 0 {
            return self.write_word_aligned(address, value);
        }
        let bytes = value.to_le_bytes();
        self.write_byte(address, bytes[0] as i8)?;
        self.write_byte(address.wrapping_add(1), bytes[1] as i8)?;
        self.write_byte(address.wrapping_add(2), bytes[2] as i8)?;
        self.write_byte(address.wrapping_add(3), bytes[3] as i8)?;
        Ok(())
    }

    /// Write a 32-bit word to memory (aligned addresses only).
    fn write_word_aligned(&mut self, address: u32, value: i32) -> Result<(), EmulatorError> {
        if address == 0 {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 4,
                kind: MemoryAccessKind::Write,
                pc: 0,
                regs: [0; 32],
            });
        }

        if self.in_ram(address) {
            let offset = (address - self.ram_start) as usize;
            if offset + 4 > self.ram.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 4,
                    kind: MemoryAccessKind::Write,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            let bytes = value.to_le_bytes();
            self.ram[offset] = bytes[0];
            self.ram[offset + 1] = bytes[1];
            self.ram[offset + 2] = bytes[2];
            self.ram[offset + 3] = bytes[3];
            return Ok(());
        }

        #[cfg(feature = "std")]
        if self.in_shared(address) {
            let arc = self.shared_backing.as_ref().unwrap();
            let mut v = arc.lock().unwrap();
            let offset = (address - self.shared_start) as usize;
            if offset + 4 > v.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 4,
                    kind: MemoryAccessKind::Write,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            let bytes = value.to_le_bytes();
            v[offset] = bytes[0];
            v[offset + 1] = bytes[1];
            v[offset + 2] = bytes[2];
            v[offset + 3] = bytes[3];
            return Ok(());
        }

        Err(EmulatorError::InvalidMemoryAccess {
            address,
            size: 4,
            kind: MemoryAccessKind::Write,
            pc: 0,
            regs: [0; 32],
        })
    }

    /// Read a byte from memory.
    pub fn read_byte(&self, address: u32) -> Result<i8, EmulatorError> {
        Ok(self.read_u8(address)? as i8)
    }

    /// Read a halfword (16-bit) from memory.
    pub fn read_halfword(&self, address: u32) -> Result<i16, EmulatorError> {
        if address % 2 != 0 && !self.allow_unaligned_access {
            return Err(EmulatorError::UnalignedAccess {
                address,
                alignment: 2,
                pc: 0,
                regs: [0; 32],
            });
        }
        if address % 2 == 0 {
            return self.read_halfword_aligned(address);
        }
        let b0 = self.read_u8(address)?;
        let b1 = self.read_u8(address.wrapping_add(1))?;
        Ok(i16::from_le_bytes([b0, b1]))
    }

    /// Read a halfword from memory (aligned addresses only).
    fn read_halfword_aligned(&self, address: u32) -> Result<i16, EmulatorError> {
        if self.in_ram(address) {
            let offset = (address - self.ram_start) as usize;
            if offset + 2 > self.ram.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 2,
                    kind: MemoryAccessKind::Read,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            let bytes = [self.ram[offset], self.ram[offset + 1]];
            return Ok(i16::from_le_bytes(bytes));
        }
        #[cfg(feature = "std")]
        if self.in_shared(address) {
            let arc = self.shared_backing.as_ref().unwrap();
            let v = arc.lock().unwrap();
            let offset = (address - self.shared_start) as usize;
            if offset + 2 > v.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 2,
                    kind: MemoryAccessKind::Read,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            let bytes = [v[offset], v[offset + 1]];
            return Ok(i16::from_le_bytes(bytes));
        }
        let offset = (address - self.code_start) as usize;
        if offset + 2 > self.code.len() {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 2,
                kind: MemoryAccessKind::Read,
                pc: 0,
                regs: [0; 32],
            });
        }
        let bytes = [self.code[offset], self.code[offset + 1]];
        Ok(i16::from_le_bytes(bytes))
    }

    /// Write a byte to memory.
    pub fn write_byte(&mut self, address: u32, value: i8) -> Result<(), EmulatorError> {
        if address == 0 {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 1,
                kind: MemoryAccessKind::Write,
                pc: 0,
                regs: [0; 32],
            });
        }

        if self.in_ram(address) {
            let offset = (address - self.ram_start) as usize;
            if offset >= self.ram.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 1,
                    kind: MemoryAccessKind::Write,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            self.ram[offset] = value as u8;
            return Ok(());
        }

        #[cfg(feature = "std")]
        if self.in_shared(address) {
            let arc = self.shared_backing.as_ref().unwrap();
            let mut v = arc.lock().unwrap();
            let offset = (address - self.shared_start) as usize;
            if offset >= v.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 1,
                    kind: MemoryAccessKind::Write,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            v[offset] = value as u8;
            return Ok(());
        }

        Err(EmulatorError::InvalidMemoryAccess {
            address,
            size: 1,
            kind: MemoryAccessKind::Write,
            pc: 0,
            regs: [0; 32],
        })
    }

    /// Write a halfword (16-bit) to memory.
    pub fn write_halfword(&mut self, address: u32, value: i16) -> Result<(), EmulatorError> {
        if address % 2 != 0 && !self.allow_unaligned_access {
            return Err(EmulatorError::UnalignedAccess {
                address,
                alignment: 2,
                pc: 0,
                regs: [0; 32],
            });
        }
        if address % 2 == 0 {
            return self.write_halfword_aligned(address, value);
        }
        let bytes = value.to_le_bytes();
        self.write_byte(address, bytes[0] as i8)?;
        self.write_byte(address.wrapping_add(1), bytes[1] as i8)?;
        Ok(())
    }

    /// Write a halfword to memory (aligned addresses only).
    fn write_halfword_aligned(&mut self, address: u32, value: i16) -> Result<(), EmulatorError> {
        if address == 0 {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 2,
                kind: MemoryAccessKind::Write,
                pc: 0,
                regs: [0; 32],
            });
        }

        if self.in_ram(address) {
            let offset = (address - self.ram_start) as usize;
            if offset + 2 > self.ram.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 2,
                    kind: MemoryAccessKind::Write,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            let bytes = value.to_le_bytes();
            self.ram[offset] = bytes[0];
            self.ram[offset + 1] = bytes[1];
            return Ok(());
        }

        #[cfg(feature = "std")]
        if self.in_shared(address) {
            let arc = self.shared_backing.as_ref().unwrap();
            let mut v = arc.lock().unwrap();
            let offset = (address - self.shared_start) as usize;
            if offset + 2 > v.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 2,
                    kind: MemoryAccessKind::Write,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            let bytes = value.to_le_bytes();
            v[offset] = bytes[0];
            v[offset + 1] = bytes[1];
            return Ok(());
        }

        Err(EmulatorError::InvalidMemoryAccess {
            address,
            size: 2,
            kind: MemoryAccessKind::Write,
            pc: 0,
            regs: [0; 32],
        })
    }

    /// Read a 32-bit instruction from the code region.
    ///
    /// For compressed instructions (RVC), this may return a 16-bit value in the lower 16 bits.
    /// Returns an error if the address is out of bounds or not 2-byte aligned.
    pub fn fetch_instruction(&self, address: u32) -> Result<u32, EmulatorError> {
        if address % 2 != 0 {
            return Err(EmulatorError::UnalignedAccess {
                address,
                alignment: 2,
                pc: 0,
                regs: [0; 32],
            });
        }

        if self.in_shared(address) {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 2,
                kind: MemoryAccessKind::InstructionFetch,
                pc: 0,
                regs: [0; 32],
            });
        }

        let (data, offset) = if self.in_ram(address) {
            let offset = (address - self.ram_start) as usize;
            if offset + 2 > self.ram.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 2,
                    kind: MemoryAccessKind::InstructionFetch,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            (&self.ram, offset)
        } else {
            let offset = (address - self.code_start) as usize;
            if offset + 2 > self.code.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 2,
                    kind: MemoryAccessKind::InstructionFetch,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            (&self.code, offset)
        };

        let first_half = u16::from_le_bytes([data[offset], data[offset + 1]]);

        if (first_half & 0x3) != 0x3 {
            Ok(first_half as u32)
        } else {
            if offset + 4 > data.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 4,
                    kind: MemoryAccessKind::InstructionFetch,
                    pc: 0,
                    regs: [0; 32],
                });
            }

            let bytes = [
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ];
            Ok(u32::from_le_bytes(bytes))
        }
    }

    /// Read a single byte from memory.
    pub fn read_u8(&self, address: u32) -> Result<u8, EmulatorError> {
        if self.in_ram(address) {
            let offset = (address - self.ram_start) as usize;
            if offset >= self.ram.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 1,
                    kind: MemoryAccessKind::Read,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            return Ok(self.ram[offset]);
        }
        #[cfg(feature = "std")]
        if self.in_shared(address) {
            let arc = self.shared_backing.as_ref().unwrap();
            let v = arc.lock().unwrap();
            let offset = (address - self.shared_start) as usize;
            if offset >= v.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 1,
                    kind: MemoryAccessKind::Read,
                    pc: 0,
                    regs: [0; 32],
                });
            }
            return Ok(v[offset]);
        }
        let offset = (address - self.code_start) as usize;
        if offset >= self.code.len() {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 1,
                kind: MemoryAccessKind::Read,
                pc: 0,
                regs: [0; 32],
            });
        }
        Ok(self.code[offset])
    }

    /// Get a reference to the RAM region (for inspection).
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Get a mutable reference to the RAM region (for initialization).
    pub fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }

    /// Get a reference to the code region (for debugging).
    pub fn code(&self) -> &[u8] {
        &self.code
    }

    /// Get the base address of the code region.
    pub fn code_start(&self) -> u32 {
        self.code_start
    }

    /// Get the base address of the shared region (when present).
    pub fn shared_start(&self) -> u32 {
        self.shared_start
    }

    /// Whether a shared backing buffer is installed.
    pub fn has_shared_region(&self) -> bool {
        self.has_shared()
    }

    /// Get the base address of the RAM region.
    pub fn ram_start(&self) -> u32 {
        self.ram_start
    }

    /// Get the end address of the RAM region (exclusive).
    pub fn ram_end(&self) -> u32 {
        self.ram_start.wrapping_add(self.ram.len() as u32)
    }

    /// Exclusive end of the shared region in guest address space.
    #[cfg(feature = "std")]
    pub fn shared_end(&self) -> Option<u32> {
        if !self.has_shared() {
            return None;
        }
        Some(
            self.shared_start
                .wrapping_add(self.shared_slice_len() as u32),
        )
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use alloc::vec;

    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn shared_memory_read_write_word() {
        let shared = Arc::new(Mutex::new(vec![0u8; 1024]));
        let mut mem = Memory::new_with_shared(
            vec![],
            vec![0u8; 256],
            shared.clone(),
            0x0,
            DEFAULT_SHARED_START,
            DEFAULT_RAM_START,
        );
        mem.write_word(DEFAULT_SHARED_START, 0x1122_3344).unwrap();
        assert_eq!(mem.read_word(DEFAULT_SHARED_START).unwrap(), 0x1122_3344);
        let g = shared.lock().unwrap();
        assert_eq!(i32::from_le_bytes([g[0], g[1], g[2], g[3]]), 0x1122_3344);
    }

    #[test]
    fn backward_compatible_no_shared() {
        let mem = Memory::with_default_addresses(vec![], vec![0u8; 64]);
        assert!(!mem.has_shared_region());
    }

    #[test]
    fn fetch_from_shared_fails() {
        let shared = Arc::new(Mutex::new(vec![0u8; 64]));
        let mem = Memory::new_with_shared(
            vec![],
            vec![0u8; 64],
            shared,
            0x0,
            DEFAULT_SHARED_START,
            DEFAULT_RAM_START,
        );
        assert!(mem.fetch_instruction(DEFAULT_SHARED_START).is_err());
    }
}
