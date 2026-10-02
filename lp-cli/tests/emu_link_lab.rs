//! The comms lab on the emulated C6: `lp-link` inside the `test_comms_lab`
//! image, the lab's host half here, the USB link between them damaged by the
//! emulator's fault injector (`lp_emu_esp_common::link_faults`), all in
//! EMULATED time (`lp-emu:esp32c6:t1`). Plan `reliable-device-link`, M3.
//!
//! The claim, per fault mix: every soak message on a reliable channel arrives
//! whole, once and in order (both directions, checked message by message);
//! the link never resets; and the host link's count of damaged frames tracks
//! what the injector did to the device → host direction.
//!
//! `#[ignore]`d: it needs the `test_comms_lab` ELF (`LP_EMU_BUILD_FW=1`
//! builds it) and runs a minute of emulation. `just link-lab-emu` runs it.
//! It lives in lp-cli because the host half is lp-cli's (`link lab emu:`),
//! and nothing under `lp-emu/` may depend on lp-link.

use lp_cli::commands::link::lab_run::{EmuLab, LabOutcome, run_emu};
use lp_emu_esp_common::link_faults::LinkFaults;
use lp_emu_esp32c6::machine::TimeGrade;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lp_link::LinkConfig;
use lp_link::lab::LabPlan;

const LAB: FwImage = FwImage {
    features: &["esp32c6", "test_comms_lab"],
    default_features: false,
};

#[test]
#[ignore = "needs the test_comms_lab ELF and a minute of emulation; `just link-lab-emu` runs it"]
fn the_link_loses_nothing_the_app_can_see_under_injected_faults() {
    let elf = match fw_esp32c6_image(&LAB) {
        Ok(p) => p,
        Err(reason) => {
            eprintln!("emu_link_lab: skipped — {reason}");
            return;
        }
    };
    let mixes = [
        ("clean", ""),
        (
            "M1 shape: 0.1% ~1 KB runs",
            "in-run=0.1%,run-packets=16,seed=11",
        ),
        (
            "1% mixed each way",
            "in-drop=0.5%,in-tail=0.5%,in-corrupt=0.05%,out-drop=0.5%,out-tail=0.5%,seed=12",
        ),
        (
            "5% mixed each way",
            "in-drop=2.5%,in-tail=2.5%,in-corrupt=0.1%,out-drop=2.5%,out-tail=2.5%,seed=13",
        ),
    ];
    for (name, spec) in mixes {
        let faults = LinkFaults::parse(spec).expect("a fault spec");
        let o = run(&elf, faults, 0);
        eprintln!("\n=== {name} ===\n{}", summary(&o));
        assert!(o.report.problems().is_empty(), "{name}: {}", o.report);
        assert_no_lost_wakes(name, &o);
        if let Some((to_host, _)) = o.faults {
            let damaged = to_host.packets_dropped
                + to_host.tails_cut
                + to_host.bits_flipped
                + to_host.runs_started;
            let seen = u64::from(o.host.bad_frames + o.host.stale_partials);
            eprintln!(
                "{name}: {damaged} device→host packets damaged, host saw {seen} damaged frames"
            );
            if damaged > 20 {
                // One damaged packet ruins at most the frame(s) it touches;
                // a lost delimiter can merge two frames into one bad one.
                assert!(
                    seen * 3 >= damaged && seen <= damaged * 3,
                    "{name}: host counted {seen} damaged frames for {damaged} damaged packets"
                );
            }
        }
    }
}

#[test]
#[ignore = "needs the test_comms_lab ELF; `just link-lab-emu` runs it"]
fn a_long_board_stall_is_ridden_out() {
    let Ok(elf) = fw_esp32c6_image(&LAB) else {
        return;
    };
    let faults = LinkFaults::parse("in-drop=0.5%,out-drop=0.5%,seed=21").unwrap();
    let o = run(&elf, faults, 3_000);
    eprintln!("\n=== 3 s board stall, 0.5% drops ===\n{}", summary(&o));
    assert!(o.report.problems().is_empty(), "{}", o.report);
    assert_no_lost_wakes("3 s board stall", &o);
    assert_eq!(o.report.stall_asked_ms, 3_000);
}

fn run(elf: &std::path::Path, faults: LinkFaults, stall_ms: u32) -> LabOutcome {
    let plan = LabPlan {
        echo_for: 8_000_000,
        stream_for: 8_000_000,
        logs: 200,
        stall_ms,
        seed: 5,
        ..LabPlan::default()
    };
    run_emu(
        &EmuLab {
            elf,
            faults: Some(faults),
            free_lag_ns: 0,
            grade: TimeGrade::T1,
            slice_us: 250,
            blockprof: false,
        },
        plan,
        LinkConfig::usb(),
    )
    .expect("the emulated lab runs")
}

/// No USB write waited out its timeout with a host draining and the send
/// buffer already free: the esp-hal interrupt handler's lost TX wake
/// (docs/defects/2026-09-26-esp-hals-usb-isr-clears-a-tx-edge-it-did-not-handle.md),
/// fixed by the upstream back-port in `third_party/esp-hal`. Stock esp-hal
/// 1.1.1 lost about five a minute here.
fn assert_no_lost_wakes(name: &str, o: &LabOutcome) {
    let lost = o
        .report
        .board
        .iter()
        .find(|(k, _)| k == "edge.lost_wakes")
        .map(|(_, v)| *v)
        .unwrap_or_else(|| panic!("{name}: the board reported no edge.lost_wakes"));
    assert_eq!(lost, 0, "{name}: {lost} USB write(s) lost their wake-up");
}

fn summary(o: &LabOutcome) -> String {
    let mut s = format!(
        "{} ({:.1} s emulated)\n{}",
        o.configuration, o.seconds, o.report
    );
    let h = &o.host;
    s += &format!(
        "host link: {} resent ({} timer, {} early, {} probes), {} damaged frames, {} stale partials\n",
        h.retransmits, h.timeouts, h.fast_retransmits, h.probes, h.bad_frames, h.stale_partials
    );
    if let Some((a, b)) = &o.faults {
        s += &format!("injected: device→host {a}; host→device {b}\n");
    }
    s
}
