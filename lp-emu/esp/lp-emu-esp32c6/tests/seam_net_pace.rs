//! A run's pace on the C6 (`Esp32C6Builder::pace`,
//! `lp_emu_esp_common::seam::net::lan_pace`): a set pace reaches the board's
//! LAN when the network seam engages and is in the configuration label; an
//! unset one changes nothing; and `realtime`, which is held at the LAN pump,
//! is refused wherever there is no LAN to hold it with (but not on a blank
//! chip, which runs nothing to pace until it is flashed).
//!
//! Nothing here runs a paced board for any length of time, and nothing reads
//! the host's clock: the pace's rule is `lan_host_pace`'s tests, and which
//! boards it holds is `shared_lan`'s.

mod seam_guest;

use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::SeamRequest;
use lp_emu_esp_common::seam::net::{LanDriver, Pace, SharedLan};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{BootMode, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition};
use seam_guest::net_guest::*;
use seam_guest::*;

#[test]
fn a_set_pace_is_on_the_lan_and_in_the_label_and_an_unset_one_is_neither() {
    let m = board(None, None);
    assert_eq!(m.configuration_label(), "lp-emu:esp32c6:t1+net=lan");
    assert_eq!(m.lan().unwrap().pace(m.net_endpoint_id()), None);

    for (pace, label) in [
        (Pace::Realtime, "lp-emu:esp32c6:t1+net=lan@pace=realtime"),
        (Pace::Max, "lp-emu:esp32c6:t1+net=lan@pace=max"),
    ] {
        // On the board's own LAN, and on one its host gave it.
        let given = SharedLan::new(fixture_lan(), LanDriver::SelfDriven);
        for lan in [None, Some(given)] {
            let m = board(lan, Some(pace));
            assert_eq!(m.configuration_label(), label);
            assert_eq!(m.lan().unwrap().pace(m.net_endpoint_id()), Some(pace));
        }
    }
}

#[test]
fn max_needs_no_lan_and_says_so_in_the_label() {
    let m = build(
        Esp32C6Builder::new()
            .seams(SeamRequest::none())
            .pace(Pace::Max),
    )
    .expect("max paces nothing, so it needs nothing");
    assert_eq!(m.configuration_label(), "lp-emu:esp32c6:t1@pace=max");
}

#[test]
fn realtime_is_refused_with_no_network_seam_asked_for() {
    for request in [
        SeamRequest::none(),
        SeamRequest::strict("net=real").unwrap(),
    ] {
        let err = build(Esp32C6Builder::new().seams(request).pace(Pace::Realtime))
            .err()
            .expect("realtime with no LAN to hold it");
        assert!(err.contains("pace realtime"), "{err}");
        assert!(err.contains("net=lan"), "{err}");
    }
}

#[test]
fn realtime_is_refused_on_a_runners_lan() {
    let lan = SharedLan::new(fixture_lan(), LanDriver::Runner);
    let err = build(
        Esp32C6Builder::new()
            .lan(lan, ParticipantId(0))
            .pace(Pace::Realtime),
    )
    .err()
    .expect("a runner's LAN cannot wait");
    assert!(err.contains("runner"), "{err}");
}

/// An image that carries a seam table but not the network seam's entries
/// (the foundation's test image): the default request engages nothing, so a
/// `realtime` run ends at the app's start rather than running unpaced.
#[test]
fn realtime_ends_the_run_when_the_network_seam_does_not_engage() {
    let page = core_page(Tables::ALL, &[spin()], &[]);
    let mut m = machine(
        chip(&[(CORE_A, &page)]),
        Esp32C6Builder::bare().pace(Pace::Realtime),
    );
    boot_core(&mut m, CORE_A);
    match m.run_until(&StopCondition::after_micros(200)) {
        Outcome::Seam { why, .. } => {
            assert!(why.contains("pace realtime"), "{why}");
            assert!(why.contains("did not engage"), "{why}");
        }
        other => panic!("expected the run to end on the pace, got {other:?}"),
    }
}

/// A blank chip booting ROM-up runs no app until it is flashed, so it has
/// nothing to pace yet and is not refused: the restart after a flash is the
/// chip start that checks.
#[test]
fn a_blank_rom_up_chip_is_not_refused_before_it_is_flashed() {
    let m = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash(FlashBacking::Blank)
        .pace(Pace::Realtime)
        .build()
        .map_err(|e| e.to_string())
        .expect("a blank chip builds");
    assert_eq!(m.configuration_label(), "lp-emu:esp32c6:t1@pace=realtime");
}

/// A booted synthetic board carrying the network seam, on `lan` or its own.
fn board(lan: Option<SharedLan>, pace: Option<Pace>) -> Esp32C6Machine {
    let page = net_core_page(NetPage::default());
    let mut builder = Esp32C6Builder::new();
    if let Some(lan) = lan {
        builder = builder.lan(lan, ParticipantId(0));
    }
    if let Some(pace) = pace {
        builder = builder.pace(pace);
    }
    let mut m = machine(chip(&[(CORE_A, &page)]), builder);
    boot_core(&mut m, CORE_A);
    m
}

/// Build over the network guest's chip, or say why not.
fn build(builder: Esp32C6Builder) -> Result<Esp32C6Machine, String> {
    let page = net_core_page(NetPage::default());
    builder
        .flash(FlashBacking::Bytes(chip(&[(CORE_A, &page)])))
        .build()
        .map_err(|e| e.to_string())
}
