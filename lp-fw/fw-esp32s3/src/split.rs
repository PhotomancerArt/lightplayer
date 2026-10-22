//! The S3 core's half of a split image (`LP_SPLIT_LINK=1` builds, made by
//! `lp-fw-split build --chip esp32s3`; the C6's twin is the split section
//! of `fw-esp32c6/src/main.rs`).
//!
//! One link, cut by reachability into a **core** — everything `core_boot`
//! and the RAM code reach — and an **engine**, reached only through
//! [`ENGINE_HEADER`]'s `entry`, which the core reads through a plain address.
//!
//! The S3's one MMU table serves both buses at one index (layout 2,
//! `lp_bootctl::s3_mmu`): the engine is one run of flash pages mapped at
//! `ENGINE_MMU_ENTRY`, its header and rodata read through the data view
//! (`ENGINE_DROM`), its code fetched through the instruction view of the same
//! entries (`ENGINE_IROM`).
//!
//! **Updates** (the S3 plan's M7 P8): the boot records' marks, the engine
//! guard, the engine choice, the running engine's update hooks and core-only
//! are `fw_esp32_common::split_core`'s — the C6's code, run over this chip's
//! [`S3Split`](crate::split_core::S3Split). The sequence is the C6's:
//! `split::begin` marks a trial attempted right after the partition table is
//! read (`mount_filesystem`, before Bluetooth starts); [`split_boot`] marks it
//! started, then enters the engine the records and the header agree on, or
//! runs core-only — the update session on USB and on every Bluetooth link,
//! a trial confirmed when any host link comes up.

use fw_esp32_common::split_core as split;
use lp_bootctl::engine_header::{ENGINE_BUILD_ID_LEN, ENGINE_HEADER_LEN};
use lpfs::lp_path::AsLpPath as _;

use crate::split_core::S3Split;
use crate::{CoreBoot, lp_engine_entry};

/// The engine's header, the first bytes of the engine region: the
/// `#[repr(C)]` mirror of `lp_bootctl::engine_header`'s v1 layout (the C6's
/// `EngineHeader`). The core never names it.
#[repr(C)]
struct EngineHeader {
    magic: u32,
    version: u16,
    header_len: u16,
    entry: fn(CoreBoot),
    /// Patched by the packager: `engine.bin`'s length.
    len: u32,
    build_id: [u8; ENGINE_BUILD_ID_LEN],
    /// Patched by the packager: CRC-32 of the bytes before it.
    crc: u32,
    commit: u32,
}

const _: () = {
    use core::mem::{offset_of, size_of};
    use lp_bootctl::engine_header as eh;
    assert!(offset_of!(EngineHeader, entry) == eh::ENGINE_ENTRY_OFFSET);
    assert!(offset_of!(EngineHeader, len) == eh::ENGINE_LEN_OFFSET);
    assert!(offset_of!(EngineHeader, build_id) == eh::ENGINE_BUILD_ID_OFFSET);
    assert!(offset_of!(EngineHeader, crc) == eh::ENGINE_CRC_OFFSET);
    assert!(offset_of!(EngineHeader, commit) == eh::ENGINE_COMMIT_OFFSET);
    assert!(size_of::<EngineHeader>() == eh::ENGINE_HEADER_LEN);
};

/// This build's id, `"<version>+<commit>"`: core and engine carry the same
/// one because they come from the same link. Named, so the split tool reads
/// the core's copy and checks the engine header's against it.
#[unsafe(no_mangle)]
static LP_BUILD_ID: [u8; ENGINE_BUILD_ID_LEN] = lp_bootctl::engine_header::build_id_field(
    concat!(env!("LP_APP_VERSION"), "+", env!("LP_BUILD_COMMIT")).as_bytes(),
);

/// The engine digest slot: SHA-256 of this build's `engine.bin` exactly as
/// flashed, patched in by the packager (`lp_bootctl::engine_digest`). Named
/// so the split tool finds it, and a core root there.
#[unsafe(no_mangle)]
static LP_ENGINE_DIGEST: [u8; lp_bootctl::engine_digest::ENGINE_DIGEST_LEN] =
    lp_bootctl::engine_digest::ENGINE_DIGEST_UNPATCHED;

#[unsafe(link_section = ".engine_header")]
#[used]
static ENGINE_HEADER: EngineHeader = EngineHeader {
    magic: lp_bootctl::engine_header::ENGINE_MAGIC,
    version: lp_bootctl::engine_header::ENGINE_HEADER_VERSION,
    header_len: ENGINE_HEADER_LEN as u16,
    entry: lp_engine_entry,
    len: 0,
    build_id: lp_bootctl::engine_header::build_id_field(
        concat!(env!("LP_APP_VERSION"), "+", env!("LP_BUILD_COMMIT")).as_bytes(),
    ),
    crc: 0,
    commit: lp_bootctl::engine_header::ENGINE_COMMITTED,
};

/// The digest the core carries, read volatile (the packager patched it).
fn engine_digest() -> [u8; 32] {
    // SAFETY: a plain read of a static the packager patched in flash.
    let slot = unsafe { core::ptr::read_volatile(&raw const LP_ENGINE_DIGEST) };
    lp_bootctl::engine_digest::decode(&slot).unwrap_or([0; 32])
}

/// After `core_boot`: mark a trial started, then enter the engine the header
/// and the boot records agree on, or stay core-only (the C6's `split_boot`).
pub fn split_boot(mut core: CoreBoot) {
    let mut state = core::mem::replace(&mut core.ota_state, split::BootState::placeholder());
    // Test images only (never in a build def): a trial core that dies, or
    // hangs, after marking itself attempted and before it starts — what the
    // loader's rollback and cold cap exist for.
    #[cfg(feature = "fixture-trial-dies")]
    if state.on_trial() {
        panic!("fixture-trial-dies: dying on trial before started");
    }
    #[cfg(feature = "fixture-trial-hangs")]
    if state.on_trial() {
        log::error!("fixture-trial-hangs: hanging on trial before started");
        loop {
            core::hint::spin_loop();
        }
    }
    // Links and radios are up: from here a power cycle never counts against
    // a trial.
    split::mark_started::<S3Split>(&mut state);

    // The image's identity: the engine choice checks the header and guards
    // the engine against it, and the update session reports it.
    let identity = core_identity();
    let engine = split::choose_engine::<S3Split>(&mut state, &identity);
    split::log_boot_state::<S3Split>(&state, engine.map(|(_, len)| len), &identity);
    // SAFETY: a committed header of this build, from this link: its entry
    // is `lp_engine_entry`'s address in the engine now mapped behind it.
    let engine = engine.map(|(entry, len)| {
        (
            unsafe { core::mem::transmute::<usize, fn(CoreBoot)>(entry as usize) },
            len,
        )
    });
    // What the update session needs, in both modes: the device store's
    // `secrets` and `open` (the core reads only those, and never writes the
    // file), and how far the USB link is trusted.
    let access = lpc_update::board::AccessFacts::from_file(
        &lpa_server::access_store::device_store_at_boot(core.base_fs.as_ref(), core.fs_boot_state),
    );
    let usb_trust = if cfg!(feature = "fixture-usb-untrusted") {
        log::warn!("[OTA] fixture-usb-untrusted: the USB link is NOT trusted (a test image)");
        lpc_update::board::LinkTrust::Untrusted
    } else {
        lpc_update::board::LinkTrust::Trusted
    };
    // Where the update session answers: USB, and the radio links. A radio
    // boot probe image has the shared crate's radio links but no product
    // radio: an idle port, which opens none.
    #[cfg(all(feature = "ble", not(s3_ble)))]
    let radio_port = fw_esp32_common::radio_link::RadioLinkPort::leak();
    #[cfg(s3_ble)]
    let radio_port = core.radio_port;
    let links = split::UpdateLinks {
        usb: core.usb_link,
        #[cfg(feature = "ble")]
        radio: radio_port,
    };
    // The radio links' mode follows the choice, decided here — in the same
    // synchronous run as `core_boot`, so before the BLE task has run at all,
    // and a connection that subscribes waits for it (`wait_for_mode`):
    // core-only's links advertise the wide receive window from their SYN.
    #[cfg(feature = "ble")]
    radio_port.decide_mode(match engine {
        Ok(_) => fw_esp32_common::radio_link::RadioLinkMode::Serve,
        Err(_) => fw_esp32_common::radio_link::RadioLinkMode::Update,
    });
    match engine {
        Ok((entry, len)) => {
            split::install_running_hook::<S3Split>(links, state, identity, len, access, usb_trust);
            entry(core)
        }
        Err(why) => {
            let CoreBoot {
                spawner,
                usb_link,
                watchdog,
                rmt_peripheral,
                base_fs,
                ..
            } = core;
            // The update light: the strip the engine recorded, if any.
            let record = base_fs
                .read_file(lpc_update::STATUS_LIGHT_PATH.as_path())
                .ok();
            let light = crate::split_core::StatusLight::new(record.as_deref(), rmt_peripheral);
            // Its own task, like the engine's server loop (the C6's reason:
            // awaited here, its future would sit in the main task's frame).
            spawner.spawn(
                core_only_task(split::CoreOnly {
                    usb_link,
                    #[cfg(feature = "ble")]
                    radio_port,
                    watchdog,
                    state,
                    why,
                    identity,
                    access,
                    usb_trust,
                    entropy: crate::fill_random,
                    light,
                    // No Wi-Fi on this chip yet (the S3 plan's M4): no saved
                    // network makes a trial wait on a deadline.
                    network_saved: false,
                })
                .unwrap(),
            );
        }
    }
}

/// Core-only, as a task of its own (see [`split_boot`]).
#[embassy_executor::task]
async fn core_only_task(ctx: split::CoreOnly<S3Split>) {
    split::core_only(ctx).await
}

/// The image's identity, as the update session reports it: every field from
/// the build itself (the build id static, the patched digest slot, the
/// manifest core's words).
fn core_identity() -> split::CoreIdentity {
    split::CoreIdentity {
        build_id: LP_BUILD_ID,
        digest: engine_digest(),
        target: env!("LP_FW_TARGET"),
        chip: "esp32s3",
        version: env!("LP_APP_VERSION"),
        wire_proto: lpc_wire::WIRE_PROTO_VERSION,
    }
}
