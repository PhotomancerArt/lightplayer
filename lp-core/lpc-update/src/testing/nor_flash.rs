//! An in-memory NOR flash with power cuts.
//!
//! - **Program only clears bits** (`old & new`), as NOR does.
//! - **Erase** sets a 4 KiB sector to `0xFF`; a **block erase** sets a
//!   larger aligned span to `0xFF` in one operation.
//! - Every erase and program counts as one **operation**. With
//!   [`NorFlash::cut_after`]`(k)`, operations `1..=k` happen and the flash is
//!   then **frozen**: every later operation (reads included) fails with
//!   [`FlashFault`] and changes nothing — the power is gone. With
//!   [`NorFlash::tear`], the first operation past the cut is half done (the
//!   first half of a program's bytes, the first half of an erase's sector),
//!   which is what a real cut mid-write can leave.
//! - [`NorFlash::power_on`] clears the cut: the bytes stay as they froze.

use alloc::vec;
use alloc::vec::Vec;

use crate::board::FlashFault;
use crate::code_table::CHUNK;

/// The model. See the module docs.
#[derive(Clone, Debug)]
pub struct NorFlash {
    bytes: Vec<u8>,
    ops: u64,
    cut_after: Option<u64>,
    tear: bool,
    frozen: bool,
}

impl NorFlash {
    /// `size` bytes of erased flash.
    #[must_use]
    pub fn new(size: usize) -> Self {
        Self {
            bytes: vec![0xFF; size],
            ops: 0,
            cut_after: None,
            tear: false,
            frozen: false,
        }
    }

    /// Cut the power after `k` more operations from now.
    pub fn cut_after(&mut self, k: u64) {
        self.cut_after = Some(self.ops + k);
    }

    /// Whether the cut half-does the first operation past it.
    pub fn tear(&mut self, tear: bool) {
        self.tear = tear;
    }

    /// Whether the power has been cut.
    #[must_use]
    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// Power back on: no cut armed, the bytes as they froze.
    pub fn power_on(&mut self) {
        self.cut_after = None;
        self.frozen = false;
    }

    /// Operations (erases and programs) so far.
    #[must_use]
    pub fn ops(&self) -> u64 {
        self.ops
    }

    /// The bytes, for a test's eyes only.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Write bytes directly, as a factory flasher would (no operation count).
    pub fn flash_image(&mut self, addr: u32, bytes: &[u8]) {
        let at = addr as usize;
        self.bytes[at..at + bytes.len()].copy_from_slice(bytes);
    }

    /// Erase the sector holding `addr`.
    pub fn erase(&mut self, addr: u32) -> Result<(), FlashFault> {
        self.erase_span(addr / CHUNK * CHUNK, CHUNK)
    }

    /// Erase the `len`-byte block at `addr` in one operation (a torn cut
    /// leaves its first half erased).
    pub fn erase_block(&mut self, addr: u32, len: u32) -> Result<(), FlashFault> {
        if len == 0 || addr % len != 0 {
            return Err(FlashFault);
        }
        self.erase_span(addr, len)
    }

    fn erase_span(&mut self, addr: u32, len: u32) -> Result<(), FlashFault> {
        let start = addr as usize;
        let end = start + len as usize;
        if end > self.bytes.len() {
            return Err(FlashFault);
        }
        let cut_here = self.operation()?;
        let end = if cut_here {
            start + len as usize / 2
        } else {
            end
        };
        self.bytes[start..end].fill(0xFF);
        if cut_here { Err(FlashFault) } else { Ok(()) }
    }

    /// Program `bytes` at `addr` (bits only go 1 → 0).
    pub fn program(&mut self, addr: u32, bytes: &[u8]) -> Result<(), FlashFault> {
        let at = addr as usize;
        if at + bytes.len() > self.bytes.len() {
            return Err(FlashFault);
        }
        let cut_here = self.operation()?;
        let n = if cut_here {
            bytes.len() / 2
        } else {
            bytes.len()
        };
        for (dst, src) in self.bytes[at..at + n].iter_mut().zip(bytes) {
            *dst &= *src;
        }
        if cut_here { Err(FlashFault) } else { Ok(()) }
    }

    /// Read `buf.len()` bytes at `addr`.
    pub fn read(&self, addr: u32, buf: &mut [u8]) -> Result<(), FlashFault> {
        if self.frozen {
            return Err(FlashFault);
        }
        let at = addr as usize;
        let src = self.bytes.get(at..at + buf.len()).ok_or(FlashFault)?;
        buf.copy_from_slice(src);
        Ok(())
    }

    /// Count one operation. `Err` when the power is already gone;
    /// `Ok(true)` when this is the operation the cut lands in (torn, if
    /// tearing, and then the power is gone).
    fn operation(&mut self) -> Result<bool, FlashFault> {
        if self.frozen {
            return Err(FlashFault);
        }
        if self.cut_after.is_some_and(|k| self.ops >= k) {
            self.frozen = true;
            if self.tear {
                return Ok(true);
            }
            return Err(FlashFault);
        }
        self.ops += 1;
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_only_clears_bits_and_erase_sets_them() {
        let mut f = NorFlash::new(8192);
        f.program(10, &[0x0F]).unwrap();
        f.program(10, &[0xF3]).unwrap();
        assert_eq!(f.bytes()[10], 0x03);
        f.erase(4096 + 7).unwrap();
        assert_eq!(f.bytes()[10], 0x03, "another sector");
        f.erase(0).unwrap();
        assert_eq!(f.bytes()[10], 0xFF);
        assert_eq!(f.ops(), 4);
    }

    #[test]
    fn a_cut_freezes_everything_after_it() {
        let mut f = NorFlash::new(8192);
        f.cut_after(1);
        f.program(0, &[0]).unwrap();
        assert_eq!(f.program(1, &[0]), Err(FlashFault));
        assert_eq!(f.erase(0), Err(FlashFault));
        let mut b = [0u8; 2];
        assert_eq!(f.read(0, &mut b), Err(FlashFault));
        assert!(f.is_frozen());
        f.power_on();
        f.read(0, &mut b).unwrap();
        assert_eq!(b, [0x00, 0xFF], "the second program never happened");
    }

    #[test]
    fn a_torn_cut_half_does_the_operation() {
        let mut f = NorFlash::new(8192);
        f.tear(true);
        f.cut_after(0);
        assert_eq!(f.program(0, &[0; 8]), Err(FlashFault));
        f.power_on();
        assert_eq!(&f.bytes()[..8], &[0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF]);
    }
}
