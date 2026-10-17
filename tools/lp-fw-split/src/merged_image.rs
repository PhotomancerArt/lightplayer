//! `merged.bin`: the whole 4 MiB chip as a flasher writes it — the IDF
//! second-stage bootloader at `0x0`, the partition table at `0x8000`, and
//! `app.bin` at the app partition's start, erased (`0xFF`) everywhere else.
//!
//! espflash's own merge does the first two: it is given the loader's ELF as
//! the app, with the partition table and the flash settings
//! `scripts/emu/build-merged-image.sh` uses (DIO, 40 MHz, 4 MB — the silicon
//! transcript's own words), and the bootloader [`C6_BOOTLOADER`] (the
//! `esp32c6-4mb` build def's; espflash patches its header with the same flash
//! settings). `app.bin` is then written over the app partition.

use std::path::Path;

use anyhow::{Result, anyhow, bail};
use espflash::elf::ElfFirmwareImage;
use espflash::flasher::{FlashData, FlashFrequency, FlashMode, FlashSettings, FlashSize};
use espflash::targets::{Chip, XtalFrequency};
use lp_bootctl::LOADER_OFFSET;

/// The bootloader a split image carries unless told otherwise: espflash
/// 3.3.0's bundled one, rebuilt from source and committed byte-identical
/// (`lp-fw/bootloaders/README.md`). Repo-relative; the `esp32c6-4mb` build
/// def names the same file (a test in lp-cli holds the two together).
pub const C6_BOOTLOADER: &str = "lp-fw/bootloaders/esp32c6-bootloader-idf-v5.1.bin";

/// The C6's flash: 4 MiB.
pub const FLASH_LEN: usize = 4 * 1024 * 1024;

/// `merged` with its second stage replaced: `bootloader` at `0x0`, erased
/// (`0xFF`) to the partition table at `0x8000`. Moving a board between the
/// two front doors rewrites exactly this range; everything from the
/// partition table on is the same bytes.
pub fn with_bootloader(merged: &[u8], bootloader: &[u8]) -> Result<Vec<u8>> {
    const TABLE: usize = 0x8000;
    if bootloader.len() > TABLE {
        bail!("the bootloader runs into the partition table");
    }
    let mut out = merged.to_vec();
    out[..TABLE].fill(0xff);
    out[..bootloader.len()].copy_from_slice(bootloader);
    Ok(out)
}

/// The merged image of `app` over the bootloader and `partitions`.
/// `loader_elf` stands in as espflash's app; `bootloader` is the second-stage
/// bootloader to merge (`None`: the one espflash bundles).
pub fn merge(
    loader_elf: &[u8],
    app: &[u8],
    partitions: &Path,
    bootloader: Option<&Path>,
) -> Result<Vec<u8>> {
    let image = ElfFirmwareImage::try_from(loader_elf).map_err(|e| anyhow!("{e}"))?;
    let flash_data = FlashData::new(
        bootloader,
        Some(partitions),
        None,
        None,
        FlashSettings::new(
            Some(FlashMode::Dio),
            Some(FlashSize::_4Mb),
            Some(FlashFrequency::_40Mhz),
        ),
        0,
    )
    .map_err(|e| anyhow!("{e}"))?;
    let built = Chip::Esp32c6
        .into_target()
        .get_flash_image(&image, flash_data, None, XtalFrequency::_40Mhz)
        .map_err(|e| anyhow!("{e}"))?;
    let mut out = Vec::with_capacity(FLASH_LEN);
    let mut app_at = None;
    for segment in built.flash_segments() {
        let addr = segment.addr as usize;
        if addr < out.len() {
            bail!("espflash's segments overlap at {addr:#x}");
        }
        out.resize(addr, 0xff);
        out.extend_from_slice(&segment.data);
        if segment.addr == LOADER_OFFSET {
            app_at = Some(addr);
        }
    }
    let Some(app_at) = app_at else {
        bail!("the partition table does not put the app at {LOADER_OFFSET:#x}");
    };
    if app_at + app.len() > FLASH_LEN {
        bail!("app.bin runs past the end of the flash");
    }
    out.resize(FLASH_LEN.max(out.len()), 0xff);
    out[app_at..app_at + app.len()].copy_from_slice(app);
    if out.len() != FLASH_LEN {
        bail!("the merged image is {} B, not {FLASH_LEN}", out.len());
    }
    Ok(out)
}
