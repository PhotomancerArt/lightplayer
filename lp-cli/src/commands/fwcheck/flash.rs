//! In-process ESP32 flashing for fwcheck via espflash-as-a-library.
//!
//! fwcheck flashes locally BUILT check firmware (an ELF straight out of
//! `cargo build`, features vary per check), so it drives espflash's
//! ELF path directly rather than the link provider's manifest-based
//! `manage()` (which flashes the packaged studio firmware). Semantics match
//! the espflash CLI invocation this module replaced:
//! `espflash flash --chip esp32c6 --partition-table <csv> --after <mode>
//! [--erase-parts lpfs] <elf>`.

use std::path::Path;

use crate::client::esp32_probe::usb_port_info_for;
use anyhow::{Context, Result, bail};
use espflash::connection::reset::{ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::{FlashData, FlashSettings, Flasher, parse_partition_table};
use espflash::targets::{Chip, XtalFrequency};

const CHIP: Chip = Chip::Esp32c6;
const CONNECT_BAUD: u32 = 115_200;
const PARTITION_TABLE: &str = "lp-fw/fw-esp32c6/partitions.csv";
const FW_ELF: &str = "target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6";

/// Flash the built fw-esp32c6 ELF and hard-reset into it.
pub fn flash_esp32(root: &Path, port: &str, verbose: bool) -> Result<()> {
    flash_esp32_elf(root, port, ResetAfterOperation::HardReset, false, verbose)
}

/// Flash the built fw-esp32c6 ELF with a blank `lpfs` data partition, leaving
/// the chip in the bootloader. The demo monitor opens the port with
/// `reset_after_open`, so the first application boot happens under the line
/// observer and every boot log is captured.
pub fn flash_esp32_no_reset_erase_lpfs(root: &Path, port: &str, verbose: bool) -> Result<()> {
    flash_esp32_elf(root, port, ResetAfterOperation::NoReset, true, verbose)
}

fn flash_esp32_elf(
    root: &Path,
    port: &str,
    after: ResetAfterOperation,
    erase_lpfs: bool,
    verbose: bool,
) -> Result<()> {
    let elf_path = root.join(FW_ELF);
    let elf = std::fs::read(&elf_path)
        .with_context(|| format!("read firmware ELF {}", elf_path.display()))?;
    let partition_table = root.join(PARTITION_TABLE);

    // The layout preflight (the C6 repartition, MQ7): a table that differs
    // from the board's, in either direction, would strand or destroy the
    // board's files. The demo erases its filesystem anyway — there the
    // board's OWN region goes too, so no old filesystem is left for the new
    // firmware to hold; a check run refuses.
    let image_table = lpa_link::PartitionTable::from_csv(
        &std::fs::read_to_string(&partition_table)
            .with_context(|| format!("read {}", partition_table.display()))?,
    )
    .map_err(|error| anyhow::anyhow!("{}: {error}", partition_table.display()))?;
    let (_, device_table) = lpa_link::providers::host_serial_esp32::read_partition_table(
        port,
        &lpa_link::LinkManagementEventSink::noop(),
    )
    .map_err(|error| anyhow::anyhow!("layout preflight: {error}"))?;
    let preflight = lpa_link::layout_migration::preflight(&device_table, &image_table);
    let device_lpfs = lpa_link::PartitionTable::parse(&device_table)
        .ok()
        .as_ref()
        .and_then(lpa_link::LinkFlashRegion::lpfs_in);
    if let lpa_link::layout_migration::LayoutPreflight::Differs { from, to } = &preflight
        && !erase_lpfs
    {
        bail!(
            "layout preflight: the board holds the {from}; this image writes the {to}. \
             Flashing would strand the board's files. Move them first (`lp-cli hardware lpfs \
             migrate --port {port} --merged <image>`) or, on a test board, erase them \
             (`lp-cli hardware lpfs preflight --port {port} --table {} --discard-lpfs`).",
            partition_table.display()
        );
    }

    let mut flasher = connect(port, after)?;

    if erase_lpfs
        && !preflight.allows_plain_flash()
        && let Some(region) = device_lpfs
    {
        if verbose {
            println!(
                "erasing the board's own lpfs @ 0x{:x} (0x{:x} bytes): its layout differs",
                region.offset, region.length
            );
        }
        flasher
            .erase_region(region.offset, region.length)
            .context("erase the board's lpfs partition")?;
    }

    if erase_lpfs {
        let table = parse_partition_table(&partition_table)
            .with_context(|| format!("parse partition table {}", partition_table.display()))?;
        let Some(lpfs) = table.find("lpfs") else {
            bail!(
                "partition table {} has no `lpfs` partition",
                partition_table.display()
            );
        };
        if verbose {
            println!(
                "erasing lpfs @ 0x{:x} (0x{:x} bytes)",
                lpfs.offset(),
                lpfs.size()
            );
        }
        flasher
            .erase_region(lpfs.offset(), lpfs.size())
            .context("erase lpfs partition")?;
    }

    let flash_data = FlashData::new(
        None,
        Some(&partition_table),
        None,
        None,
        FlashSettings::default(),
        0,
    )
    .context("prepare flash data")?;
    let mut progress = FlashProgress { verbose };
    // The flash target's `finish` applies the requested `--after` behavior
    // (hard-reset into the app, or stay in the bootloader) itself; calling
    // `reset_after` again would talk to a stub that is already gone.
    flasher
        .load_elf_to_flash(
            &elf,
            flash_data,
            Some(&mut progress),
            XtalFrequency::default(CHIP),
        )
        .context("flash firmware ELF")?;
    Ok(())
}

fn connect(port: &str, after: ResetAfterOperation) -> Result<Flasher> {
    let serial = serialport::new(port, CONNECT_BAUD)
        .flow_control(serialport::FlowControl::None)
        .open_native()
        .with_context(|| format!("open serial port {port}"))?;
    Flasher::connect(
        serial,
        usb_port_info_for(port),
        Some(CONNECT_BAUD),
        /* use_stub  */ true,
        /* verify    */ false,
        /* skip      */ false,
        Some(CHIP),
        after,
        ResetBeforeOperation::DefaultReset,
    )
    .context("espflash connect")
}

struct FlashProgress {
    verbose: bool,
}

impl espflash::flasher::ProgressCallbacks for FlashProgress {
    fn init(&mut self, addr: u32, total: usize) {
        if self.verbose {
            println!("writing 0x{addr:x} ({total} chunks)");
        }
    }

    fn update(&mut self, _current: usize) {}

    fn finish(&mut self) {
        if self.verbose {
            println!("segment done");
        }
    }
}
