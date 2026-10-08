//! `embedded-storage`'s blocking NOR traits over [`NorFlashSim`], so ecosystem
//! stores (`sequential-storage`, …) run on the same model as ours.
//!
//! READ_SIZE and WRITE_SIZE are 1; ERASE_SIZE is 4096, so these impls assume
//! the C6's 4 KiB sectors (checked on every erase).

use embedded_storage::nor_flash::{
    ErrorType, MultiwriteNorFlash, NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash,
};

use crate::{NorError, NorFlashSim};

/// [`NorError`] as an `embedded-storage` error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NorSimFlashError(pub NorError);

impl NorFlashError for NorSimFlashError {
    fn kind(&self) -> NorFlashErrorKind {
        match self.0 {
            NorError::OutOfBounds => NorFlashErrorKind::OutOfBounds,
            NorError::PowerLost | NorError::Watchdog => NorFlashErrorKind::Other,
        }
    }
}

impl ErrorType for NorFlashSim {
    type Error = NorSimFlashError;
}

impl ReadNorFlash for NorFlashSim {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        NorFlashSim::read(self, offset, bytes).map_err(NorSimFlashError)
    }

    fn capacity(&self) -> usize {
        self.geometry().capacity() as usize
    }
}

impl NorFlash for NorFlashSim {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = 4096;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        let ss = self.geometry().sector_size;
        assert_eq!(
            ss as usize,
            Self::ERASE_SIZE,
            "embedded-storage impl needs 4 KiB sectors"
        );
        if from % ss != 0 || to % ss != 0 || to < from {
            return Err(NorSimFlashError(NorError::OutOfBounds));
        }
        for s in from / ss..to / ss {
            self.erase_sector(s).map_err(NorSimFlashError)?;
        }
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.program(offset, bytes).map_err(NorSimFlashError)
    }
}

impl MultiwriteNorFlash for NorFlashSim {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FaultPlan, NorGeometry, TearModel};

    #[test]
    fn traits_round_trip() {
        let mut f = NorFlashSim::filled(NorGeometry::new(4, 4096, 256), 0);
        NorFlash::erase(&mut f, 4096, 8192).unwrap();
        NorFlash::write(&mut f, 4100, b"hello").unwrap();
        let mut b = [0u8; 5];
        ReadNorFlash::read(&mut f, 4100, &mut b).unwrap();
        assert_eq!(&b, b"hello");
        assert_eq!(ReadNorFlash::capacity(&f), 4 * 4096);
        assert!(NorFlash::erase(&mut f, 1, 4096).is_err());
    }

    #[test]
    fn power_lost_surfaces_as_other() {
        let mut f = NorFlashSim::new(NorGeometry::new(1, 4096, 256));
        f.set_plan(FaultPlan::cut(0, TearModel::Clean, 0));
        let e = NorFlash::write(&mut f, 0, b"x").unwrap_err();
        assert_eq!(e.kind(), NorFlashErrorKind::Other);
        assert_eq!(e.0, NorError::PowerLost);
    }
}
