//! Raw flash access for the update path: the region the boot records, the
//! core images and the engine live in, outside any filesystem.

use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};

pub const SECTOR: u32 = 4096;

/// A word-aligned 4 KiB buffer: the flash driver refuses unaligned buffers.
#[repr(C, align(4))]
pub struct SectorBuf(pub [u8; SECTOR as usize]);

/// The SPI flash, for the update path only.
pub struct SplitFlash(esp_storage::FlashStorage<'static>);

impl SplitFlash {
    /// SAFETY of the steal: the update path runs either core-only (the
    /// engine, and so lpfs's only writer, never started this boot) or from
    /// the server loop between frames (lpfs not mid-operation).
    pub fn take() -> Self {
        Self(esp_storage::FlashStorage::new(unsafe {
            esp_hal::peripherals::FLASH::steal()
        }))
    }

    pub fn read(&mut self, at: u32, out: &mut [u8]) -> bool {
        self.0.read(at, out).is_ok()
    }

    pub fn erase(&mut self, at: u32) {
        if let Err(e) = self.0.erase(at, at + SECTOR) {
            esp_println::println!("[OTA] erase {at:#x} failed: {e:?}");
        }
    }

    /// Erase the sector at `at` and write `data` (≤ 4 KiB) at its start.
    pub fn write_sector(&mut self, buf: &mut SectorBuf, at: u32, data: &[u8]) {
        self.erase(at);
        buf.0.fill(0xff);
        buf.0[..data.len()].copy_from_slice(data);
        let len = (data.len() + 3) & !3;
        if let Err(e) = self.0.write(at, &buf.0[..len]) {
            esp_println::println!("[OTA] write {at:#x} failed: {e:?}");
        }
    }

    /// Program one word in place, without an erase: only 1 → 0 bits change.
    pub fn program_word(&mut self, at: u32, word: u32) {
        #[repr(C, align(4))]
        struct Word([u8; 4]);
        let w = Word(word.to_le_bytes());
        if let Err(e) = self.0.write(at, &w.0) {
            esp_println::println!("[OTA] program {at:#x} failed: {e:?}");
        }
    }
}
