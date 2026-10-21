//! The Xtensa seam ABI on a real build (S3 radio M5): the `test_seam_abi`
//! harness image, built with `release-esp32s3`'s LTO and `opt-level = "z"`,
//! calls `test_echo(1, 2, 4)`, drains `test_take` and — when the network
//! seam's engaged byte says so — `net_mac` and `net_link`, through the shims
//! `lp_seam::seam_fn!` generates for Xtensa (a windowed `callx8` through a
//! volatile function pointer). The emulator answers at a `break` planted
//! over each function's `entry`, before the window rotates.
//!
//! - Seams off, the silicon bodies run: `echo=7`, both engaged bytes read
//!   false, and nothing was answered.
//! - With `test=echo+test=take` engaged the printed value is the emulator's
//!   answer (`0x5ea00007`), so the arguments reached the seam function from
//!   the caller's `a10..a12` and the result came back through its `a10`; the
//!   take copies what the host queued into the buffer the call handed over.
//! - With the default request (`net=lan`, softly) the network seam engages:
//!   `net_mac` writes the eFuse MAC through its out-pointer, `net_link`
//!   answers 0 on an empty LAN, and the label says `+net=lan`.
//!
//! - The wake: with the take engaged the harness then binds its wake
//!   handler and sleeps in `waiti`; bytes the host queues raise
//!   `SYSTEM.cpu_intr_from_cpu_3`, the handler swaps the pending word, and
//!   the harness drains exactly what was queued, once per raise.
//!
//! Every run checks the canary — a value the harness keeps live across every
//! call — so an answer that returned into the wrong window fails here, not
//! somewhere later.
//!
//! `#[ignore]`d: needs the harness image (`LP_EMU_ESP32S3_SEAM_ABI_ELF`);
//! `just test-emu-esp32s3-boot` builds it and runs this with
//! `--features test-seams`. A missing image skips loudly.

#![cfg(feature = "test-seams")]

use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::{EndpointEvent, EndpointId, SeamRequest};
use lp_emu_esp32s3::loader::EfuseIdentity;
use lp_emu_esp32s3::machine::{AppSource, Esp32S3Builder, Machine, StopCondition, UsbHost};
use lp_emu_esp32s3::seams::TEST_ECHO_MARK;
use lp_emu_esp32s3::test_support::{seam_abi_image, skip_notice};

const DONE: &str = "[SEAM-ABI] done";
const CANARY: &str = "[SEAM-ABI] canary=0x5eedc0de";
/// A locally administered MAC for the harness's eFuse, so `net_mac`'s answer
/// cannot be mistaken for an untouched zeroed buffer.
const MAC: [u8; 6] = [0x02, 0x5e, 0xa3, 0x53, 0x33, 0x01];

#[test]
#[ignore = "needs the test_seam_abi harness image; `just test-emu-esp32s3-boot`"]
fn the_harness_prints_silicons_answers_with_the_seams_off() {
    let Some(mut m) = harness("seams off", SeamRequest::none()) else {
        return;
    };
    let text = run_to_done(&mut m);
    assert!(has_line(&text, "[SEAM-ABI] echo=7"), "{text}");
    assert!(has_line(&text, "[SEAM-ABI] take engaged=false"), "{text}");
    assert!(has_line(&text, "[SEAM-ABI] net engaged=false"), "{text}");
    assert_eq!(m.seams().calls, 0);
    assert_eq!(m.seams().scans, 0, "a seam-off run never scans its flash");
    assert_eq!(m.configuration_label(), "lp-emu:esp32s3:t1");
}

#[test]
#[ignore = "needs the test_seam_abi harness image; `just test-emu-esp32s3-boot`"]
fn the_harness_prints_the_emulators_answers_with_the_seams_on() {
    // `net=real`: this test is about the value and take ABI alone. With the
    // default `net=lan` beside them the network's endpoint would take index 0
    // and `test_take(0, …)` would name it instead of the test endpoint.
    let Some(mut m) = harness(
        "seams on",
        SeamRequest::strict("test=echo+test=take+net=real").unwrap(),
    ) else {
        return;
    };
    // The image resolves its seams once core 0 first runs from the flash
    // window; queue the take's bytes as soon as its endpoint exists.
    let id = EndpointId {
        board: ParticipantId(0),
        seam: "test",
    };
    let mut us = 0;
    while m.seam_endpoint(id).is_none() {
        us += 50;
        assert!(us < 200_000, "the seams never resolved");
        m.run_until(&StopCondition::after_micros(us));
    }
    let at = m.cycles();
    m.seam_endpoint_mut(id)
        .unwrap()
        .push_inbound(EndpointEvent {
            at,
            bytes: vec![0xde, 0xad, 0xbe, 0xef],
        })
        .unwrap();
    let text = run_to_done(&mut m);
    let echo = TEST_ECHO_MARK | 7;
    assert!(
        has_line(&text, &format!("[SEAM-ABI] echo={echo}")),
        "the emulator's answer reached the caller through its a10:\n{text}"
    );
    assert!(has_line(&text, "[SEAM-ABI] take engaged=true"), "{text}");
    assert!(
        has_line(&text, "[SEAM-ABI] take 4 B: de ad be ef"),
        "the take filled the call's buffer:\n{text}"
    );
    assert!(has_line(&text, "[SEAM-ABI] net engaged=false"), "{text}");
    assert!(
        m.seams().calls >= 3,
        "echo, a take with bytes, an empty take"
    );
    assert_eq!(
        m.configuration_label(),
        "lp-emu:esp32s3:t1+test=echo+test=take"
    );
}

#[test]
#[ignore = "needs the test_seam_abi harness image; `just test-emu-esp32s3-boot`"]
fn the_network_seam_engages_by_default_and_answers_its_calls() {
    let Some(mut m) = harness("net=lan", SeamRequest::default()) else {
        return;
    };
    // The S3 machine's default eFuse MAC is all zeros, which a buffer the
    // answer never wrote would print too: the harness runs with MAC below.
    let text = run_to_done(&mut m);
    assert!(has_line(&text, "[SEAM-ABI] echo=7"), "only net=lan: {text}");
    assert!(has_line(&text, "[SEAM-ABI] net engaged=true"), "{text}");
    let mac = MAC;
    let hex: Vec<String> = mac.iter().map(|b| format!("{b:02x}")).collect();
    assert!(
        has_line(&text, &format!("[SEAM-ABI] net mac 1: {}", hex.join(" "))),
        "net_mac wrote the eFuse MAC through its out-pointer:\n{text}"
    );
    assert!(
        has_line(&text, "[SEAM-ABI] net link=0"),
        "an empty LAN has no link:\n{text}"
    );
    assert_eq!(m.configuration_label(), "lp-emu:esp32s3:t1+net=lan");
    assert!(m.lan().is_some(), "an engaged board has a LAN of its own");
    assert!(m.seams().calls >= 2, "net_mac and net_link");
}

#[test]
#[ignore = "needs the test_seam_abi harness image; `just test-emu-esp32s3-boot`"]
fn the_wake_reaches_a_sleeping_core_and_delivers_each_queue_once() {
    let Some(mut m) = harness(
        "wake",
        SeamRequest::strict("test=echo+test=take+net=real").unwrap(),
    ) else {
        return;
    };
    let waiting = "[SEAM-ABI] waiting for wakes";
    let stop = StopCondition {
        exit_on: Some(waiting.to_string()),
        ..StopCondition::after_micros(2_000_000)
    };
    m.run_until(&stop);
    assert!(console(&m).contains(waiting), "{}", console(&m));
    let id = EndpointId {
        board: ParticipantId(0),
        seam: "test",
    };
    for (n, bytes) in [vec![1u8, 2, 3], vec![4]].into_iter().enumerate() {
        let at = m.cycles();
        m.seam_endpoint_mut(id)
            .expect("the test endpoint")
            .push_inbound(EndpointEvent {
                at,
                bytes: bytes.clone(),
            })
            .unwrap();
        let until = m.micros() + 5_000;
        m.run_until(&StopCondition::after_micros(until));
        let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let line = format!(
            "[SEAM-ABI] woke {}: take {} B: {}",
            n + 1,
            bytes.len(),
            hex.join(" ")
        );
        assert!(has_line(&console(&m), &line), "{line}:\n{}", console(&m));
    }
    // Nothing queued: no raise, no wake, and the core sleeps.
    let until = m.micros() + 20_000;
    m.run_until(&StopCondition::after_micros(until));
    let text = console(&m);
    assert!(!text.contains("[SEAM-ABI] woke 3"), "{text}");
    let stats = &m.seams().wake_stats[0];
    assert_eq!((stats.raised, stats.consumed), (2, 2), "{stats:?}");
    assert_eq!(stats.bytes_taken, 4);
    assert!(m.first_strict_violation().is_none());
    for line in m.seam_wake_lines() {
        println!("{line}");
    }
}

fn console(m: &Machine) -> String {
    String::from_utf8_lossy(&m.usb_sj()).into_owned()
}

fn harness(what: &str, request: SeamRequest) -> Option<Machine> {
    let elf = match seam_abi_image() {
        Ok(path) => path,
        Err(reason) => {
            skip_notice(&format!("seam_abi_harness ({what})"), &reason);
            return None;
        }
    };
    Some(
        Esp32S3Builder::new()
            .app(AppSource::Path(elf))
            .usb_host(UsbHost::Attached { draining: true })
            .efuse(EfuseIdentity {
                mac: MAC,
                ..EfuseIdentity::default()
            })
            .seams(request)
            .build()
            .expect("the harness image builds a machine"),
    )
}

fn has_line(text: &str, line: &str) -> bool {
    text.lines().any(|l| l.trim_end() == line)
}

fn run_to_done(m: &mut Machine) -> String {
    let stop = StopCondition {
        exit_on: Some(DONE.to_string()),
        ..StopCondition::after_micros(2_000_000)
    };
    m.run_until(&stop);
    let text = String::from_utf8_lossy(&m.usb_sj()).into_owned();
    assert!(text.contains(DONE), "the harness ran to its end:\n{text}");
    assert!(
        has_line(&text, CANARY),
        "a value live across every call came back intact:\n{text}"
    );
    assert!(m.first_strict_violation().is_none());
    text
}
