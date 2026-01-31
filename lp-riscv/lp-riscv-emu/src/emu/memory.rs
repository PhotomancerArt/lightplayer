//! Memory model for the RISC-V 32 emu.

use alloc::vec::Vec;

use super::error::{EmulatorError, MemoryAccessKind};

/// Default RAM start address (0x80000000, matching embive's RAM_OFFSET).
pub const DEFAULT_RAM_START: u32 = 0x80000000;

/// Memory model with separate code and RAM regions.
pub struct Memory {
    code: Vec<u8>,
    ram: Vec<u8>,
    code_start: u32,
    ram_start: u32,
}

impl Memory {
    /// Create a new memory model.
    ///
    /// # Arguments
    ///
    /// * `code` - Code region (read-only)
    /// * `ram` - RAM region (read-write)
    /// * `code_start` - Base address for code (typically 0x0)
    /// * `ram_start` - Base address for RAM (typically 0x80000000)
    pub fn new(code: Vec<u8>, ram: Vec<u8>, code_start: u32, ram_start: u32) -> Self {
        Self {
            code,
            ram,
            code_start,
            ram_start,
        }
    }

    /// Create a new memory model with default addresses.
    pub fn with_default_addresses(code: Vec<u8>, ram: Vec<u8>) -> Self {
        Self::new(code, ram, 0x0, DEFAULT_RAM_START)
    }

    /// Read a 32-bit word from memory.
    ///
    /// Returns an error if the address is out of bounds or unaligned.
    pub fn read_word(&self, address: u32) -> Result<i32, EmulatorError> {
        // Check alignment
        if address % 4 != 0 {
            return Err(EmulatorError::UnalignedAccess {
                address,
                alignment: 4,
                pc: 0, // Will be filled in by caller
                regs: [0; 32],
            });
        }

        // Determine which region
        if address >= self.ram_start {
            // RAM region
            let offset = (address - self.ram_start) as usize;
            if offset + 4 > self.ram.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 4,
                    kind: MemoryAccessKind::Read,
                    pc: 0, // Will be filled in by caller
                    regs: [0; 32],
                });
            }
            let bytes = [
                self.ram[offset],
                self.ram[offset + 1],
                self.ram[offset + 2],
                self.ram[offset + 3],
            ];
            Ok(i32::from_le_bytes(bytes))
        } else {
            // Code region
            let offset = (address - self.code_start) as usize;
            if offset + 4 > self.code.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 4,
                    kind: MemoryAccessKind::Read,
                    pc: 0, // Will be filled in by caller
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
    }

    /// Write a 32-bit word to memory.
    ///
    /// Returns an error if the address is out of bounds, unaligned, or in the code region.
    pub fn write_word(&mut self, address: u32, value: i32) -> Result<(), EmulatorError> {
        // Check alignment
        if address % 4 != 0 {
            return Err(EmulatorError::UnalignedAccess {
                address,
                alignment: 4,
                pc: 0, // Will be filled in by caller
                regs: [0; 32],
            });
        }

        // Prevent writes to address 0 (null pointer)
        if address == 0 {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 4,
                kind: MemoryAccessKind::Write,
                pc: 0, // Will be filled in by caller
                regs: [0; 32],
            });
        }

        // Only allow writes to RAM region
        if address < self.ram_start {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 4,
                kind: MemoryAccessKind::Write,
                pc: 0, // Will be filled in by caller
                regs: [0; 32],
            });
        }

        let offset = (address - self.ram_start) as usize;
        if offset + 4 > self.ram.len() {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 4,
                kind: MemoryAccessKind::Write,
                pc: 0, // Will be filled in by caller
                regs: [0; 32],
            });
        }

        let bytes = value.to_le_bytes();
        self.ram[offset] = bytes[0];
        self.ram[offset + 1] = bytes[1];
        self.ram[offset + 2] = bytes[2];
        self.ram[offset + 3] = bytes[3];
        Ok(())
    }

    /// Read a byte from memory.
    pub fn read_byte(&self, address: u32) -> Result<i8, EmulatorError> {
        // Determine which region
        if address >= self.ram_start {
            // RAM region
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
            Ok(self.ram[offset] as i8)
        } else {
            // Code region
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
            Ok(self.code[offset] as i8)
        }
    }

    /// Read a halfword (16-bit) from memory.
    pub fn read_halfword(&self, address: u32) -> Result<i16, EmulatorError> {
        // Check alignment
        if address % 2 != 0 {
            return Err(EmulatorError::UnalignedAccess {
                address,
                alignment: 2,
                pc: 0,
                regs: [0; 32],
            });
        }

        // Determine which region
        if address >= self.ram_start {
            // RAM region
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
            Ok(i16::from_le_bytes(bytes))
        } else {
            // Code region
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
    }

    /// Write a byte to memory.
    pub fn write_byte(&mut self, address: u32, value: i8) -> Result<(), EmulatorError> {
        // Prevent writes to address 0 (null pointer)
        if address == 0 {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 1,
                kind: MemoryAccessKind::Write,
                pc: 0,
                regs: [0; 32],
            });
        }

        // Only allow writes to RAM region
        if address < self.ram_start {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 1,
                kind: MemoryAccessKind::Write,
                pc: 0,
                regs: [0; 32],
            });
        }

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
        Ok(())
    }

    /// Write a halfword (16-bit) to memory.
    pub fn write_halfword(&mut self, address: u32, value: i16) -> Result<(), EmulatorError> {
        // Check alignment
        if address % 2 != 0 {
            return Err(EmulatorError::UnalignedAccess {
                address,
                alignment: 2,
                pc: 0,
                regs: [0; 32],
            });
        }

        // Prevent writes to address 0 (null pointer)
        if address == 0 {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 2,
                kind: MemoryAccessKind::Write,
                pc: 0,
                regs: [0; 32],
            });
        }

        // Only allow writes to RAM region
        if address < self.ram_start {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 2,
                kind: MemoryAccessKind::Write,
                pc: 0,
                regs: [0; 32],
            });
        }

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
        Ok(())
    }

    /// Read a 32-bit instruction from the code region.
    ///
    /// For compressed instructions (RVC), this may return a 16-bit value in the lower 16 bits.
    /// Returns an error if the address is out of bounds or not 2-byte aligned.
    pub fn fetch_instruction(&self, address: u32) -> Result<u32, EmulatorError> {
        // Check 2-byte alignment (required for compressed instructions)
        if address % 2 != 0 {
            return Err(EmulatorError::UnalignedAccess {
                address,
                alignment: 2,
                pc: 0, // Will be filled in by caller
                regs: [0; 32],
            });
        }

        let offset = (address - self.code_start) as usize;

        // First, read at least 2 bytes to check if it's compressed
        if offset + 2 > self.code.len() {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 2,
                kind: MemoryAccessKind::InstructionFetch,
                pc: 0, // Will be filled in by caller
                regs: [0; 32],
            });
        }

        // Read first 2 bytes
        let first_half = u16::from_le_bytes([self.code[offset], self.code[offset + 1]]);

        // Check if it's a compressed instruction (bits [1:0] != 0b11)
        if (first_half & 0x3) != 0x3 {
            // It's a compressed instruction, return 16-bit value as u32
            Ok(first_half as u32)
        } else {
            // It's a 32-bit instruction, read all 4 bytes
            if offset + 4 > self.code.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 4,
                    kind: MemoryAccessKind::InstructionFetch,
                    pc: 0, // Will be filled in by caller
                    regs: [0; 32],
                });
            }

            let bytes = [
                self.code[offset],
                self.code[offset + 1],
                self.code[offset + 2],
                self.code[offset + 3],
            ];
            Ok(u32::from_le_bytes(bytes))
        }
    }

    /// Read a single byte from memory.
    pub fn read_u8(&self, address: u32) -> Result<u8, EmulatorError> {
        if address >= self.ram_start {
            // RAM region
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
            Ok(self.ram[offset])
        } else {
            // Code region
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
}
