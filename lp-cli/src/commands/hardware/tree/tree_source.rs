//! Where `hardware tree` gets its bytes: a raw image file, or a board's
//! partition read over the bootloader (`lpfs`'s own read, read only).

use std::path::Path;

use anyhow::{Context, Result, bail};
use lp_tree_store::StoreImage;
use lpa_link::{LinkFlashRegion, PARTITION_TABLE_LEN, PARTITION_TABLE_OFFSET, PartitionTable};

use super::super::args::TreeSourceArgs;
use super::super::lpfs::lpfs_target::stderr_events;

/// A whole 4 MiB C6 chip image.
const CHIP_LEN: usize = 0x40_0000;

/// The partition's bytes and what to call them.
pub struct LoadedImage {
    pub label: String,
    pub bytes: Vec<u8>,
}

/// Read the partition `source` names. `twice` reads a board a second time
/// (for `check`'s re-read comparison) and returns that read too; for an
/// image file it is `None`.
pub fn load(source: &TreeSourceArgs, twice: bool) -> Result<(LoadedImage, Option<Vec<u8>>)> {
    match (&source.port, &source.image) {
        (Some(port), None) => {
            let first = read_board(port)?;
            let second = if twice { Some(read_board(port)?) } else { None };
            Ok((
                LoadedImage {
                    label: format!("{port} ({})", first.1),
                    bytes: first.0,
                },
                second.map(|s| s.0),
            ))
        }
        (None, Some(path)) => Ok((load_image_file(path)?, None)),
        (None, None) => bail!("give a source: --port or --image"),
        (Some(_), Some(_)) => bail!("give one source: --port or --image, not both"),
    }
}

/// An image file: one partition, or a whole chip whose own table says where
/// the partition is.
pub fn load_image_file(path: &Path) -> Result<LoadedImage> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let label = path.display().to_string();
    if bytes.len() == CHIP_LEN {
        let at = PARTITION_TABLE_OFFSET as usize;
        let table = PartitionTable::parse(&bytes[at..at + PARTITION_TABLE_LEN])
            .map_err(|e| anyhow::anyhow!("{label}: {e}"))?;
        let region =
            LinkFlashRegion::lpfs_in(&table).context("the chip's table has no lpfs row")?;
        let start = region.offset as usize;
        let end = start + region.length as usize;
        return Ok(LoadedImage {
            label: format!("{label} (chip image, lpfs at {start:#x})"),
            bytes: bytes[start..end].to_vec(),
        });
    }
    Ok(LoadedImage { label, bytes })
}

/// A board's `lpfs` partition over the bootloader. NEVER RUN against a
/// board in the M8 build session (no board was touched); `lpfs save` and
/// `lpfs report` use the same call.
fn read_board(port: &str) -> Result<(Vec<u8>, String)> {
    let read = lpa_link::providers::host_serial_esp32::read_raw_filesystem(port, &stderr_events())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok((
        read.image,
        read.chip_name.unwrap_or_else(|| "board".to_string()),
    ))
}

/// Open `bytes` with the user's sector size, turning the library's errors
/// into messages.
pub fn open_image<'a>(bytes: &'a [u8], source: &TreeSourceArgs) -> Result<StoreImage<'a>> {
    StoreImage::open(bytes, source.sector_size).map_err(|e| match e {
        lp_tree_store::ImageError::TooSmall => {
            anyhow::anyhow!(
                "the image is smaller than one sector ({} bytes)",
                bytes.len()
            )
        }
        lp_tree_store::ImageError::BadSectorSize(s) => {
            anyhow::anyhow!("--sector-size {s}: the format allows a power of two from 512 to 32768")
        }
    })
}
