//! Which chip a split build is for, and every fact of the pipeline that
//! differs between chips.
//!
//! The pipeline is one: two passes, the reachability walk, the verifier, the
//! cut, the loader, `app.bin`, `merged.bin`. A [`ChipProfile`] says how each
//! step reads and writes for one chip — the linker's spelling and its map,
//! how a relocation names its target, where the engine is linked and how
//! its pieces fold into `engine.bin`, the image builder, the page and the
//! region. The ESP32-C6's profile reproduces the pipeline as it was before
//! there were profiles, byte for byte; the ESP32-S3's is the C6's shape with
//! only what the chip forces changed:
//!
//! - **The linker is GNU ld** (`xtensa-esp32s3-elf-gcc -Wl,…`), whose map is
//!   not lld's ([`crate::gnu_map`]) and whose `--emit-relocs` output keeps
//!   stale addends, so targets are read from the linked bytes
//!   ([`crate::xtensa_targets`]).
//! - **`l32r` reaches only 256 KiB backwards**, so an engine function's
//!   literal pool is placed right before its code ([`crate::literal_pools`]).
//! - **Two views of one engine.** The S3's MMU maps entry `n` behind both
//!   `0x3C00_0000 + n·64 KiB` (data) and `0x4200_0000 + n·64 KiB`
//!   (instructions). The engine's header and rodata are linked at the data
//!   view, its code at the instruction view of the next byte, so one run of
//!   flash pages mapped once serves both.
//! - **64 KiB MMU pages**, and so layout 2's region start (`factory +
//!   0x1_0000`).
//! - **The IDF door only**: our own second stage is not built for the S3.

use std::ops::Range;

use espflash::flasher::FlashSize;
use espflash::targets::Chip;

use crate::segment_rules::SegmentRules;

/// A chip the split pipeline builds for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitChip {
    #[default]
    Esp32c6,
    Esp32s3,
}

impl SplitChip {
    pub fn profile(self) -> &'static ChipProfile {
        match self {
            Self::Esp32c6 => &ESP32C6,
            Self::Esp32s3 => &ESP32S3,
        }
    }

    /// The chip word (the build def's `chip.name`).
    pub fn name(self) -> &'static str {
        self.profile().name
    }
}

impl std::str::FromStr for SplitChip {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "esp32c6" => Ok(Self::Esp32c6),
            "esp32s3" => Ok(Self::Esp32s3),
            _ => Err(format!(
                "no split profile for chip `{s}` (esp32c6 or esp32s3)"
            )),
        }
    }
}

/// How the firmware is linked: which linker reads the arguments and writes
/// the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Linker {
    /// `rust-lld`, given its arguments directly (`--emit-relocs`, `-Map=`).
    Lld,
    /// GNU ld through `xtensa-esp32s3-elf-gcc`: arguments behind `-Wl,`.
    GnuLd,
}

/// The instruction set: how a relocation names its target, and whether
/// code has literal pools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Isa {
    /// RV32: `S + A` (`section_graph::read_relocations`).
    Rv32,
    /// Xtensa: targets from the linked bytes
    /// ([`crate::xtensa_targets`]), and every function's literal pool
    /// placed before it ([`crate::literal_pools`]).
    Xtensa,
}

/// Where the engine is linked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineRegion {
    /// One window holds the engine's rodata and its code (the C6's flash
    /// window serves both buses).
    Unified { base: u32, window: u32 },
    /// The engine's header and rodata at `data`, its code at the
    /// instruction view `code_bus − data_bus` above, both the same MMU
    /// entries (the S3).
    TwoViews {
        data: u32,
        code: u32,
        window: u32,
        /// `0x4200_0000 − 0x3C00_0000`: the distance between the two views
        /// of one MMU entry.
        bus_distance: u32,
    },
}

impl EngineRegion {
    /// The address `engine.bin` starts at (the header's).
    pub fn base(self) -> u32 {
        match self {
            Self::Unified { base, .. } => base,
            Self::TwoViews { data, .. } => data,
        }
    }

    /// Whether `addr` lies in the engine's region, in either view.
    pub fn contains(self, addr: u64) -> bool {
        match self {
            Self::Unified { base, window } => {
                (u64::from(base)..u64::from(base) + u64::from(window)).contains(&addr)
            }
            Self::TwoViews {
                data, code, window, ..
            } => {
                (u64::from(data)..u64::from(data) + u64::from(window)).contains(&addr)
                    || (u64::from(code)..u64::from(code) + u64::from(window)).contains(&addr)
            }
        }
    }

    /// An address folded onto the view `engine.bin` is laid out in: the
    /// instruction view's addresses less the bus distance. Flash addresses
    /// of the core fold the same way, so one comparison against
    /// [`Self::base`] splits core from engine.
    pub fn fold(self, addr: u64) -> u64 {
        match self {
            Self::Unified { .. } => addr,
            Self::TwoViews { bus_distance, .. } => {
                if (INSTRUCTION_BUS).contains(&addr) {
                    addr - u64::from(bus_distance)
                } else {
                    addr
                }
            }
        }
    }
}

/// The S3's instruction-bus window (`0x4200_0000..0x4400_0000`).
const INSTRUCTION_BUS: Range<u64> = 0x4200_0000..0x4400_0000;

/// Every fact of the split pipeline that differs between chips.
#[derive(Debug)]
pub struct ChipProfile {
    pub chip: SplitChip,
    /// The chip word, as the build defs and the manifest core say it.
    pub name: &'static str,
    /// The firmware crate, repo-relative; its directory is where it links.
    pub crate_dir: &'static str,
    /// Its package (and binary) name.
    pub package: &'static str,
    pub target: &'static str,
    pub profile: &'static str,
    pub default_features: &'static str,
    pub linker: Linker,
    pub isa: Isa,
    /// The partition table the image is flashed with, repo-relative.
    pub partitions: &'static str,
    /// The MMU page every extent is aligned to.
    pub page: u32,
    /// The region's first byte (`lp_bootctl`'s layout).
    pub region_start: u32,
    /// The layout integer (`lpc-update`'s code table).
    pub layout: u32,
    pub engine: EngineRegion,
    /// Whether the generated script also moves the core's `.text` up to
    /// its rodata (the C6 esp-hal fork's `_lp_text_follows_rodata`).
    pub text_follows_rodata: bool,
    /// The loader crate, repo-relative, and its package name.
    pub loader_dir: &'static str,
    pub loader_package: &'static str,
    /// Whether the loader crate builds our own second stage
    /// (`--features bootloader`).
    pub our_bootloader: bool,
    /// The IDF bootloader the IDF door carries, repo-relative (`None`:
    /// espflash's bundled one).
    pub idf_bootloader: Option<&'static str>,
    pub espflash_chip: Chip,
    pub flash_size: FlashSize,
    /// The whole chip's bytes (`merged.bin`'s length).
    pub flash_len: usize,
    /// Whether the shipped image must carry the emulator seam table (the C6
    /// does; Xtensa seams are the S3 roadmap's M5).
    pub requires_seam_table: bool,
    /// What the loader can place of a core image.
    pub segments: &'static SegmentRules,
}

/// The ESP32-C6: the pipeline as it shipped (layout 1).
pub static ESP32C6: ChipProfile = ChipProfile {
    chip: SplitChip::Esp32c6,
    name: "esp32c6",
    crate_dir: "lp-fw/fw-esp32c6",
    package: "fw-esp32c6",
    target: "riscv32imac-unknown-none-elf",
    profile: "release-esp32",
    default_features: "esp32c6,server",
    linker: Linker::Lld,
    isa: Isa::Rv32,
    partitions: "lp-fw/fw-esp32c6/partitions.csv",
    page: 0x8000,
    region_start: lp_bootctl::REGION_START,
    layout: 1,
    engine: EngineRegion::Unified {
        base: 0x4240_0000,
        window: 0x40_0000,
    },
    text_follows_rodata: true,
    loader_dir: "lp-fw/fw-esp32c6-loader",
    loader_package: "fw-esp32c6-loader",
    our_bootloader: true,
    idf_bootloader: Some("lp-fw/bootloaders/esp32c6-bootloader-idf-v5.1.bin"),
    espflash_chip: Chip::Esp32c6,
    flash_size: FlashSize::_4Mb,
    flash_len: 4 * 1024 * 1024,
    requires_seam_table: true,
    segments: &crate::segment_rules::ESP32C6_SEGMENTS,
};

/// Layout 2, the S3's (`docs`: the M7 plan's ruling L3).
///
/// ⚠️ Re-typed here until `lp-bootctl` carries layout 2 (the S3 roadmap's
/// M7 P2: `LAYOUT_2_PAGE`, `LAYOUT_2_REGION_START`, the `s3_mmu` plan's
/// `ENGINE_DROM`/`ENGINE_IROM`). When it lands, these become references.
pub mod layout_2 {
    /// The S3's only MMU page (the ROM's `Cache_Ibus_MMU_Set` refuses
    /// others).
    pub const LAYOUT_2_PAGE: u32 = 0x1_0000;
    /// `factory + 0x1_0000`: the first 64 KiB page after the boot records.
    pub const LAYOUT_2_REGION_START: u32 = 0x2_0000;
    /// The engine's data view: MMU entry `0x100`.
    pub const ENGINE_DROM: u32 = 0x3D00_0000;
    /// The engine's instruction view: the same entry.
    pub const ENGINE_IROM: u32 = 0x4300_0000;
    /// The engine window: entries `0x100..0x1C0`.
    pub const ENGINE_WINDOW: u32 = 0xC0 * LAYOUT_2_PAGE;
    /// The loader's own RAM, data view (code at the instruction alias).
    pub const LOADER_DRAM: core::ops::Range<u32> = 0x3FCB_0000..0x3FCC_0000;
}

/// The ESP32-S3: layout 2, the IDF door.
pub static ESP32S3: ChipProfile = ChipProfile {
    chip: SplitChip::Esp32s3,
    name: "esp32s3",
    crate_dir: "lp-fw/fw-esp32s3",
    package: "fw-esp32s3",
    target: "xtensa-esp32s3-none-elf",
    profile: "release-esp32s3",
    default_features: "esp32s3,server",
    linker: Linker::GnuLd,
    isa: Isa::Xtensa,
    partitions: "lp-fw/fw-esp32s3/partitions.csv",
    page: layout_2::LAYOUT_2_PAGE,
    region_start: layout_2::LAYOUT_2_REGION_START,
    layout: 2,
    engine: EngineRegion::TwoViews {
        data: layout_2::ENGINE_DROM,
        code: layout_2::ENGINE_IROM,
        window: layout_2::ENGINE_WINDOW,
        bus_distance: 0x4200_0000 - 0x3C00_0000,
    },
    text_follows_rodata: false,
    loader_dir: "lp-fw/fw-esp32s3-loader",
    loader_package: "fw-esp32s3-loader",
    our_bootloader: false,
    idf_bootloader: None,
    espflash_chip: Chip::Esp32s3,
    flash_size: FlashSize::_8Mb,
    flash_len: 8 * 1024 * 1024,
    requires_seam_table: false,
    segments: &crate::segment_rules::ESP32S3_SEGMENTS,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_c6_profile_is_the_pipeline_as_it_shipped() {
        let p = SplitChip::Esp32c6.profile();
        assert_eq!(p.target, crate::pass_link::TARGET);
        assert_eq!(p.profile, crate::pass_link::PROFILE);
        assert_eq!(p.engine.base(), crate::engine_script::ENGINE_BASE);
        assert_eq!(p.page, crate::app_image::PAGE);
        assert_eq!(p.region_start, lp_bootctl::REGION_START);
        assert_eq!(
            p.idf_bootloader,
            Some(crate::merged_image::C6_BOOTLOADER),
            "the IDF door's bootloader"
        );
        assert_eq!(p.flash_len, crate::merged_image::FLASH_LEN);
    }

    #[test]
    fn the_s3_engine_views_share_one_mmu_entry_and_fold_onto_the_data_view() {
        let e = SplitChip::Esp32s3.profile().engine;
        assert_eq!(
            (0x3D00_0000u32 >> 16) & 0x1ff,
            (0x4300_0000u32 >> 16) & 0x1ff
        );
        assert!(e.contains(0x3D00_0000) && e.contains(0x4303_0000));
        assert!(!e.contains(0x3C01_0000) && !e.contains(0x4205_0000));
        assert_eq!(e.fold(0x4303_b89c), 0x3D03_b89c);
        assert_eq!(
            e.fold(0x4205_0020),
            0x3C05_0020,
            "core code folds below the engine"
        );
        assert_eq!(e.fold(0x3C00_0020), 0x3C00_0020);
        assert_eq!(e.fold(0x4037_8000), 0x4037_8000, "RAM is not a flash view");
    }

    #[test]
    fn chips_parse_by_their_word() {
        assert_eq!("esp32s3".parse::<SplitChip>(), Ok(SplitChip::Esp32s3));
        assert_eq!(SplitChip::Esp32c6.name(), "esp32c6");
        assert!("esp32".parse::<SplitChip>().is_err());
    }
}
