//! The flash the store runs on: geometry, read, program (clears bits), erase.

/// A NOR part as the store sees it.
///
/// `program` may only clear bits; the store never programs over bytes it has
/// not erased itself, except to *kill* a sector header or a record header
/// that did not check (programming zeros, which only clears), and to program
/// a write head's last record again with the bytes it read and checked
/// (`record_scan.rs`, settling a head's tail: nothing that reads 1 is
/// cleared). Every error is treated as "power lost": the store that saw it
/// must be dropped, never reused.
pub trait Flash {
    type Error: core::fmt::Debug;
    fn sector_count(&self) -> u32;
    fn sector_size(&self) -> u32;
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Self::Error>;
    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), Self::Error>;
    fn erase_sector(&mut self, sector: u32) -> Result<(), Self::Error>;
}

impl<F: Flash + ?Sized> Flash for &mut F {
    type Error = F::Error;
    fn sector_count(&self) -> u32 {
        (**self).sector_count()
    }
    fn sector_size(&self) -> u32 {
        (**self).sector_size()
    }
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Self::Error> {
        (**self).read(addr, buf)
    }
    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), Self::Error> {
        (**self).program(addr, data)
    }
    fn erase_sector(&mut self, sector: u32) -> Result<(), Self::Error> {
        (**self).erase_sector(sector)
    }
}

#[cfg(any(test, feature = "nor-sim"))]
impl Flash for lp_nor_sim::NorFlashSim {
    type Error = lp_nor_sim::NorError;
    fn sector_count(&self) -> u32 {
        self.geometry().sector_count
    }
    fn sector_size(&self) -> u32 {
        self.geometry().sector_size
    }
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Self::Error> {
        lp_nor_sim::NorFlashSim::read(self, addr, buf)
    }
    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), Self::Error> {
        lp_nor_sim::NorFlashSim::program(self, addr, data)
    }
    fn erase_sector(&mut self, sector: u32) -> Result<(), Self::Error> {
        lp_nor_sim::NorFlashSim::erase_sector(self, sector)
    }
}
