//! Which flash parts a core may read four bits at a time (QIO), and why it
//! stays on two (DIO) otherwise.
//!
//! The C6 boots its flash in DIO: the image header says DIO, the mask ROM and
//! the second-stage bootloader read in DIO, and a QIO header makes the ROM's
//! own load fail (docs/reports/2026-10-17-c6-flash-mode-and-first-light.md).
//! The core may then move the read path to QIO itself — `fw-esp32c6`'s
//! `flash_qio` — but only on a part this list names, and only when the
//! part's **QE bit is already set**. The core never writes a status register
//! to set it: a status write is a non-volatile write, and a power cut inside
//! one can leave protection bits behind. A part with QE clear stays in DIO.
//!
//! The list is by the whole JEDEC id, as the C6's `RDID` hardware command
//! leaves it in `SPI1.W0`: manufacturer in the low byte, then memory type,
//! then capacity. Every part here keeps QE in status register 2, bit 1
//! (status bit S9), read with command `0x35` — the common serial-NOR status
//! layout these XMC parts follow; on both desk parts SR2 reads `0x02` with
//! the mask ROM's own `0x35` path and with esptool's. A part is added
//! after a desk sitting reads its id, its status register and its eFuse flash
//! fields on silicon, and runs the read-back soak in QIO. Nothing here is
//! guessed from a part family.
//!
//! **Each chip has its own list.** The evidence is a part read on a chip's
//! own controller and a QIO soak through it, so a part read on a C6 is not
//! evidence for an S3's controller: [`QIO_PARTS`] is the C6's,
//! [`ESP32S3_QIO_PARTS`] the S3's (empty until an S3 sitting reads FS3's
//! part). [`qio_ready_on`] takes the list; [`qio_ready`] is the C6's.

/// One part the core may read in QIO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QioPart {
    /// `RDID` as `SPI1.W0` holds it: `capacity << 16 | type << 8 | manufacturer`.
    pub jedec: u32,
    /// Where it was read, for the person who adds the next one.
    pub seen_on: &'static str,
}

/// The parts read on silicon so far (2026-10-17). Both are the 4 MiB die in
/// an ESP32-C6FH4's package (eFuse `PKG_VERSION 1`, `FLASH_CAP 1`,
/// `FLASH_VENDOR 1`), both with QE set at the factory (SR2 `0x02`).
pub const QIO_PARTS: &[QioPart] = &[
    QioPart {
        jedec: 0x16_40_46,
        seen_on: "CX1, generic C6 board (XMC, second manufacturer code)",
    },
    QioPart {
        jedec: 0x16_40_20,
        seen_on: "CX2, Seeed XIAO ESP32-C6 (XMC)",
    },
];

/// The S3's parts. None yet: FS3's (a XIAO ESP32-S3 Plus, 16 MB) has not had
/// its id read on silicon, so every S3 reads in DIO until a sitting adds it
/// with the evidence above (`fw-esp32s3/src/flash_qio.rs`).
pub const ESP32S3_QIO_PARTS: &[QioPart] = &[];

/// Status register 2's QE bit (bit 1), on every part in [`QIO_PARTS`].
pub const SR2_QE: u32 = 1 << 1;

/// Why the core keeps reading in DIO. Each says so in the boot's one
/// `[flash]` line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaysDio {
    /// The part is not one a desk sitting has read.
    PartNotListed,
    /// The part's QE bit is clear, and the core does not set it.
    QeClear,
}

impl StaysDio {
    /// The words the boot line uses.
    pub fn words(self) -> &'static str {
        match self {
            StaysDio::PartNotListed => "part not on the QIO list",
            StaysDio::QeClear => "QE bit clear",
        }
    }
}

/// May a C6 core read this part in QIO? [`qio_ready_on`] with [`QIO_PARTS`].
pub fn qio_ready(jedec: u32, sr2: u32) -> Result<(), StaysDio> {
    qio_ready_on(QIO_PARTS, jedec, sr2)
}

/// May the core read this part in QIO, on a chip whose list is `parts`?
/// `jedec` as `SPI1.W0` holds it after `RDID` (the low 24 bits are used),
/// `sr2` as command `0x35` returns it.
pub fn qio_ready_on(parts: &[QioPart], jedec: u32, sr2: u32) -> Result<(), StaysDio> {
    let jedec = jedec & 0x00ff_ffff;
    if !parts.iter().any(|p| p.jedec == jedec) {
        return Err(StaysDio::PartNotListed);
    }
    if sr2 & SR2_QE == 0 {
        return Err(StaysDio::QeClear);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_desk_parts_with_qe_set_go_quad() {
        assert_eq!(qio_ready(0x16_40_46, 0x02), Ok(()));
        assert_eq!(qio_ready(0x16_40_20, 0x02), Ok(()));
    }

    #[test]
    fn a_listed_part_with_qe_clear_stays_dio() {
        assert_eq!(qio_ready(0x16_40_46, 0x00), Err(StaysDio::QeClear));
        // Other SR2 bits (SUS, CMP, LB) do not stand in for QE.
        assert_eq!(qio_ready(0x16_40_20, 0xfd), Err(StaysDio::QeClear));
    }

    #[test]
    fn an_unread_part_stays_dio_even_with_qe_set() {
        // The emulator's default chip (the ROM's own default id).
        assert_eq!(qio_ready(0x16_40_ef, 0x02), Err(StaysDio::PartNotListed));
        // Same XMC die, another capacity: not read on silicon, not listed.
        assert_eq!(qio_ready(0x17_40_20, 0x02), Err(StaysDio::PartNotListed));
        // A bus that answers nothing reads all ones.
        assert_eq!(qio_ready(0xffff_ffff, 0xff), Err(StaysDio::PartNotListed));
    }

    /// A part read on a C6 is not evidence for the S3's controller.
    #[test]
    fn the_s3_reads_dio_until_its_own_sitting() {
        assert_eq!(
            qio_ready_on(ESP32S3_QIO_PARTS, 0x16_40_46, 0x02),
            Err(StaysDio::PartNotListed)
        );
        assert!(ESP32S3_QIO_PARTS.is_empty());
    }

    #[test]
    fn only_the_low_24_bits_of_w0_are_the_id() {
        assert_eq!(qio_ready(0xab16_4046, 0x02), Ok(()));
    }
}
