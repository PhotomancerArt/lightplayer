//! The shape of the part: sectors, sector size, program page size.

/// Sector count and size, and the program page a long program is split into.
///
/// The default is the ESP32-C6 data partition the testbed targets: 128 sectors
/// of 4 KiB (512 KiB) with 256-byte program pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NorGeometry {
    pub sector_count: u32,
    pub sector_size: u32,
    pub page_size: u32,
}

impl NorGeometry {
    pub const fn new(sector_count: u32, sector_size: u32, page_size: u32) -> Self {
        Self {
            sector_count,
            sector_size,
            page_size,
        }
    }

    /// `sector_count` sectors of the C6's 4 KiB with 256-byte pages.
    pub const fn c6(sector_count: u32) -> Self {
        Self::new(sector_count, 4096, 256)
    }

    pub const fn capacity(&self) -> u32 {
        self.sector_count * self.sector_size
    }
}

impl Default for NorGeometry {
    fn default() -> Self {
        Self::c6(128)
    }
}
