//! The machine a served board runs: an emulated ESP32-C6 or ESP32-S3.
//!
//! `emu serve` held C6 boards only until the S3 updates plan's P12 (the S3's
//! updates over Bluetooth): Studio's `?emu=` and `?ble=emu` pages reach a
//! board through the door's two loopback sockets (the USB-Serial-JTAG byte
//! stream and the control channel), and the S3 machine binds the same two,
//! so the door does not change — only the board thread's machine does.
//!
//! What the S3 does not have here, it refuses by name when the serve asks
//! for it (`serve::serve`): the C6's other time grades, `--air` (the radio's
//! frame tap is the C6's) and `--lpperi-clk-en` (a C6 register). Its control
//! channel has no `pin` verb yet, so a page that holds a pad on an S3 board
//! hears that from the emulator and says so on the switch.

use std::net::SocketAddr;

use anyhow::{Result, anyhow};
use lp_emu_esp_common::seam::EndpointId;
use lp_emu_esp32c6::machine::Esp32C6Machine;
use lp_emu_esp32s3::machine::Machine as Esp32S3Machine;

use super::board::{BoardKind, BoardOptions, BoardSpec, SeamReport, working_path};
use crate::commands::emu::handler::{check_file, whole_part_len};

/// One served board's machine.
pub enum ServeMachine {
    C6(Box<Esp32C6Machine>),
    S3(Box<Esp32S3Machine>),
}

/// How one run slice ended, as the board thread needs it.
pub enum SliceEnd {
    /// The slice's wall-clock net: the ordinary end, the loop gets a turn.
    Slice,
    /// The board is over (a fault, a strict refusal, a reset it could not
    /// perform), in words.
    Stopped(String),
}

impl ServeMachine {
    /// The two loopback doors the machine bound: its byte stream and its
    /// control channel.
    pub fn doors(&self) -> Option<(SocketAddr, SocketAddr)> {
        let (bytes, control) = match self {
            Self::C6(m) => (m.usb_sj_tcp(), m.control_tcp()),
            Self::S3(m) => (m.usb_sj_tcp(), m.control_tcp()),
        };
        Some((bytes?.local_addr(), control?.local_addr()))
    }

    /// The board's endpoint on its LAN (its network seam).
    pub fn net_endpoint_id(&self) -> EndpointId {
        match self {
            Self::C6(m) => m.net_endpoint_id(),
            Self::S3(m) => m.net_endpoint_id(),
        }
    }

    /// Run until the slice's wall-clock net, or until the board stops.
    pub fn run_slice(&mut self, slice: std::time::Duration) -> SliceEnd {
        match self {
            Self::C6(m) => {
                use lp_emu_esp32c6::machine::{Outcome, StopCondition};
                let stop = StopCondition {
                    // No cycle deadline: a server has none, and `reboot()`
                    // restarts the guest's clock, so an absolute one would
                    // mean a different thing after every reset.
                    stop_cycle: None,
                    exit_on: None,
                    wall_timeout: Some(slice),
                    probes: Vec::new(),
                };
                match m.run_until(&stop) {
                    Outcome::WallTimeout { .. } => SliceEnd::Slice,
                    other => SliceEnd::Stopped(crate::commands::emu::handler::describe(&other)),
                }
            }
            Self::S3(m) => {
                use lp_emu_esp32s3::machine::{Outcome, StopCondition};
                let stop = StopCondition {
                    stop_cycle: None,
                    exit_on: None,
                    wall_timeout: Some(slice),
                    probes: Vec::new(),
                };
                match m.run_until(&stop) {
                    Outcome::WallTimeout { .. } => SliceEnd::Slice,
                    other => SliceEnd::Stopped(format!("{other:?}")),
                }
            }
        }
    }

    /// The power cut a `flash_cut=` board took, once it has fired.
    pub fn last_flash_cut(&self) -> Option<String> {
        match self {
            Self::C6(m) => m.last_flash_cut().map(|r| r.to_string()),
            Self::S3(m) => m.last_flash_cut().map(|r| r.to_string()),
        }
    }

    pub fn reboots(&self) -> u64 {
        match self {
            Self::C6(m) => m.reboots(),
            Self::S3(m) => m.reboots(),
        }
    }

    pub fn power_cycles(&self) -> u64 {
        match self {
            Self::C6(m) => m.power_cycles(),
            Self::S3(m) => m.power_cycles(),
        }
    }

    /// Whether the chip holds a bootable image at the reset vector.
    pub fn has_image_at_reset_vector(&self) -> bool {
        match self {
            Self::C6(m) => m.has_image_at_reset_vector(),
            Self::S3(m) => m.has_image_at_reset_vector(),
        }
    }

    /// Write the flash part back; `true` when it was written.
    pub fn flush_flash(&mut self) -> std::io::Result<bool> {
        match self {
            Self::C6(m) => m.flush_flash(),
            Self::S3(m) => m.flush_flash(),
        }
    }

    /// The console: every byte the host side of the USB link received.
    pub fn console_bytes(&self) -> Vec<u8> {
        match self {
            Self::C6(m) => m.usb_sj().bytes(),
            Self::S3(m) => m.usb_sj(),
        }
    }

    /// What the guest handed the IN endpoint that no host took.
    pub fn untaken_bytes(&self) -> Vec<u8> {
        match self {
            Self::C6(m) => m.usb_sj_tried().bytes(),
            Self::S3(m) => m.usb_sj_tried(),
        }
    }

    pub fn flush_frames(&mut self) {
        match self {
            Self::C6(m) => m.flush_frames(),
            Self::S3(m) => m.flush_frames(),
        }
    }

    /// This chip start's new `SEAM` lines.
    pub fn take_seam_lines(&mut self) -> Vec<String> {
        match self {
            Self::C6(m) => m.take_seam_lines(),
            Self::S3(m) => m.take_seam_lines(),
        }
    }

    /// The board's configuration label, seam lines and seam endpoints.
    pub fn seam_report(&self) -> SeamReport {
        match self {
            Self::C6(m) => SeamReport {
                label: m.configuration_label(),
                lines: m.seams().lines.clone(),
                endpoints: m
                    .seam_endpoints()
                    .iter()
                    .map(|e| e.id.to_string())
                    .collect(),
            },
            Self::S3(m) => SeamReport {
                label: m.configuration_label(),
                lines: m.seams().lines.clone(),
                endpoints: m
                    .seam_endpoints()
                    .iter()
                    .map(|e| e.id.to_string())
                    .collect(),
            },
        }
    }

    /// The C6's radio frame tap (`--air`): the S3 has none (refused at the
    /// serve's start).
    pub fn as_c6_mut(&mut self) -> Option<&mut Esp32C6Machine> {
        match self {
            Self::C6(m) => Some(m),
            Self::S3(_) => None,
        }
    }
}

/// An S3 board's machine: the C6's [`super::board`] build, on the S3's
/// builder — the same two loopback doors, the same reboot-on-reset, the
/// same identity, image and flash rules.
pub fn build_s3(spec: &BoardSpec, options: &BoardOptions) -> Result<ServeMachine> {
    use lp_emu_esp32s3::flash::FlashBacking;
    use lp_emu_esp32s3::loader::EfuseIdentity;
    use lp_emu_esp32s3::machine::{
        AppSource, BootMode, Esp32S3Builder, TimeGrade, UsbSjDrain, UsbSjSink,
    };

    let mut builder = Esp32S3Builder::new()
        .time_grade(TimeGrade::T1)
        .strict(options.strict_bus)
        .usb_host(options.usb_host)
        .usb_sj_drain(UsbSjDrain::Auto)
        .reboot_on_reset(true)
        .efuse(EfuseIdentity {
            mac: spec.mac,
            ..EfuseIdentity::default()
        })
        .usb_sj(UsbSjSink::Tcp("127.0.0.1:0".to_string()))
        .control("127.0.0.1:0")
        .seams(spec.seams.clone());
    if let Some(pace) = spec.pace {
        builder = builder.pace(pace);
    }
    if let Some(cut) = &spec.flash_cut {
        builder = builder.flash_cut(cut.clone());
    }
    if let Some(seat) = &options.lan {
        builder = builder.lan(seat.lan.clone(), seat.participant);
    }

    // The working file, seeded as the C6's is (`board::build_c6`).
    let working = match &spec.flash {
        Some(durable) => {
            let working = working_path(durable);
            let _ = std::fs::remove_file(&working);
            let source = if durable.is_file() {
                Some(durable.as_path())
            } else if matches!(spec.kind, BoardKind::RomUp) {
                spec.image.as_deref()
            } else {
                None
            };
            if let Some(source) = source {
                std::fs::copy(source, &working).map_err(|e| {
                    anyhow!(
                        "board `{}`: seeding {} from {}: {e}",
                        spec.id,
                        working.display(),
                        source.display()
                    )
                })?;
            }
            Some(working)
        }
        None => None,
    };
    builder = match spec.kind {
        BoardKind::Elf => {
            let elf = spec
                .image
                .as_deref()
                .ok_or_else(|| anyhow!("board `{}`: kind=elf needs an image", spec.id))?;
            check_file(elf, "--board")?;
            builder
                .boot_mode(BootMode::Direct)
                .app(AppSource::Path(elf.to_path_buf()))
                .flash(match &working {
                    Some(path) => FlashBacking::File(path.clone()),
                    None => FlashBacking::Blank,
                })
        }
        BoardKind::Merged => {
            let merged = spec
                .image
                .as_deref()
                .ok_or_else(|| anyhow!("board `{}`: kind=merged needs an image", spec.id))?;
            check_file(merged, "--board")?;
            builder
                .boot_mode(BootMode::RomUp)
                .flash(FlashBacking::Copy(merged.to_path_buf()))
                .flash_len(whole_part_len(merged)?)
        }
        BoardKind::RomUp => {
            let builder = builder.boot_mode(BootMode::RomUp);
            match &working {
                Some(path) if path.is_file() => builder
                    .flash(FlashBacking::File(path.clone()))
                    .flash_len(whole_part_len(path)?),
                Some(path) => builder.flash(FlashBacking::File(path.clone())),
                None => builder.flash(FlashBacking::Blank),
            }
        }
    };
    let machine = builder
        .build()
        .map_err(|e| anyhow!("board `{}`: building the machine: {e}", spec.id))?;
    Ok(ServeMachine::S3(Box::new(machine)))
}
