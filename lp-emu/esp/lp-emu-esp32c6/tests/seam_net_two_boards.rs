//! Two shipped `fw-esp32c6` boards on one virtual LAN, in lockstep (P12's
//! "two boards in lockstep", the machine side): each engages `net=lan`, takes
//! an address from the LAN's DHCP server, answers its own `lp-xxxx.local`
//! through a [`LanProbe`](lp_emu_esp_common::seam::net::LanProbe), and the
//! same run twice gives the same frame log.
//!
//! `#[ignore]`d twice over:
//!
//! - it needs the shipped ELF (`just test-emu-c6` builds it, or set
//!   `LP_EMU_C6_ELF_ESP32C6_SERVER_RADIO`; see `test_support`), and that
//!   image must **carry the network seam** — P10's firmware half. Until it
//!   does the test skips with the scan's own reason.
//! - each board must be **told to join** the fixture's `home`. The firmware
//!   joins only networks it has saved, and saving one is a wire request over
//!   the board's USB link (`lp-cli wifi add`), which this MIT crate cannot
//!   speak (the `lp-emu/` fence). `LP_EMU_C6_NET_JOINED_FLASH_<A|B>` names a
//!   flash file for each board that already holds the saved network (one a
//!   board wrote after `lp-cli wifi add`); without them the test skips after
//!   checking what it can (both boards engage the seam, each is on the LAN
//!   with its own MAC, and the two runs are byte-identical).
//!
//! The director runs it once P10 lands; the same pair driven over the USB
//! link end to end belongs to `lp-cli`'s emulator tests.

use std::net::Ipv4Addr;

use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::net::lan_dns::TYPE_A;
use lp_emu_esp_common::seam::net::{FrameRecord, LanDriver, SharedLan, net_endpoint};
use lp_emu_esp_common::seam::net::{LanConfig, VirtualAccessPoint, VirtualLan};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::loader::EfuseIdentity;
use lp_emu_esp32c6::lockstep::Lockstep;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, StopCondition, TimeGrade};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image, skip_notice};

const MACS: [[u8; 6]; 2] = [
    [0xa0, 0xf2, 0x62, 0x87, 0xb4, 0x8c],
    [0xa0, 0xf2, 0x62, 0x85, 0xa8, 0x7c],
];
/// The two boards' mDNS names (`lp-` and the station MAC's last two bytes).
const NAMES: [&str; 2] = ["lp-b48c.local", "lp-a87c.local"];
const RUN_US: u64 = 8_000_000;

#[test]
#[ignore = "needs a fw-esp32c6 image carrying the network seam (P10) and boards that saved \
            `home`; see the module docs"]
fn two_shipped_boards_each_take_an_address_and_answer_their_names_deterministically() {
    let Some(first) = run() else {
        return;
    };
    let Some(second) = run() else {
        return;
    };
    assert_eq!(first.log, second.log, "the same run twice, the same frames");
    if let Some((ips, resolved)) = first.joined {
        assert_ne!(ips[0], ips[1]);
        assert_eq!(resolved, [Some(ips[0]), Some(ips[1])]);
    }
}

struct Run {
    log: Vec<FrameRecord>,
    /// Each board's address and what the probe resolved for each name, when
    /// the boards had a network saved.
    joined: Option<([Ipv4Addr; 2], [Option<Ipv4Addr>; 2])>,
}

fn run() -> Option<Run> {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("seam_net_two_boards", &reason);
            return None;
        }
    };
    let flashes = ["A", "B"].map(|x| std::env::var(format!("LP_EMU_C6_NET_JOINED_FLASH_{x}")).ok());
    let lan = SharedLan::new(
        VirtualLan::new(LanConfig::new(memmap::CYCLES_PER_US))
            .with_access_point(VirtualAccessPoint::secured("home", "test-password-1", -45))
            .with_access_point(VirtualAccessPoint::open("cafe", -80)),
        LanDriver::Runner,
    );
    lan.with(|l| l.log_frames(true));
    let probe = lan.with(|l| l.add_probe());
    let boards = (0..2)
        .map(|i| {
            let mut b = Esp32C6Builder::new()
                .app(AppSource::Path(elf.clone()))
                .time_grade(TimeGrade::T1)
                .efuse(EfuseIdentity {
                    mac: MACS[i],
                    ..EfuseIdentity::default()
                })
                .lan(lan.clone(), ParticipantId(i));
            if let Some(path) = &flashes[i] {
                b = b.flash(FlashBacking::Bytes(
                    std::fs::read(path).expect("the flash file"),
                ));
            }
            b.build().expect("the shipped image builds a machine")
        })
        .collect::<Vec<_>>();
    let mut pair = Lockstep::new(boards)
        .unwrap()
        .with_medium(Box::new(lan.clone()));
    pair.run_until(
        RUN_US * memmap::CYCLES_PER_US / 2,
        &StopCondition::default(),
    );

    for i in 0..2 {
        let m = pair.machine_mut(ParticipantId(i)).unwrap();
        if !m
            .seams()
            .engaged_impls()
            .iter()
            .any(|s| s.atom() == "net=lan")
        {
            let why = m.seams().none_why.clone().unwrap_or_default();
            skip_notice(
                "seam_net_two_boards",
                &format!("the image does not carry the network seam yet: {why}"),
            );
            return None;
        }
        assert!(m.configuration_label().ends_with("+net=lan"));
        assert!(lan.with(|l| l.station(net_endpoint(ParticipantId(i))).is_some()));
    }

    let joined = if flashes.iter().all(Option::is_some) {
        lan.with(|l| {
            for name in NAMES {
                l.probe_mut(probe).query(name, TYPE_A);
            }
        });
        pair.run_until(RUN_US * memmap::CYCLES_PER_US, &StopCondition::default());
        let ips = [0, 1].map(|i| {
            lan.address(net_endpoint(ParticipantId(i)))
                .expect("each board took an address")
        });
        let resolved = NAMES.map(|n| lan.with(|l| l.probe(probe).resolved(n)));
        Some((ips, resolved))
    } else {
        skip_notice(
            "seam_net_two_boards",
            "LP_EMU_C6_NET_JOINED_FLASH_A/_B not set: no board has `home` saved, so the \
             address and name checks did not run",
        );
        pair.run_until(RUN_US * memmap::CYCLES_PER_US, &StopCondition::default());
        None
    };
    Some(Run {
        log: lan.with(|l| l.frame_log().to_vec()),
        joined,
    })
}
