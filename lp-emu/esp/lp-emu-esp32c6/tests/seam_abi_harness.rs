//! The value-seam rule on a real build (FD6, FD10): the `test_seam_abi`
//! harness image, built with `release-esp32`'s LTO and `opt-level = "z"`,
//! calls `test_echo(1, 2, 4)` and drains `test_take` through the shims
//! `lp_seam::seam_fn!` generated.
//!
//! - Seam off, the silicon bodies run: `echo=7`, and the take's engaged byte
//!   reads false.
//! - With `test=echo+test=take` engaged, the printed value is the emulator's
//!   answer (`0x5ea00007`), so the arguments reached the seam function and
//!   its result was read — not folded to 7, which is what M0 saw LTO do to an
//!   ordinary `#[no_mangle]` body — and the take copies what the host queued
//!   into the buffer the call handed over.
//!
//! `#[ignore]`d: needs the harness image (`LP_EMU_BUILD_FW=1`, or
//! `LP_EMU_C6_ELF_ESP32C6_TEST_SEAM_ABI`); `just test-emu-c6` builds it and
//! runs this with `--features test-seams`. A missing image skips loudly.

#![cfg(feature = "test-seams")]

use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::{EndpointEvent, EndpointId, SeamRequest};
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, Esp32C6Machine, StopCondition, UsbHost};
use lp_emu_esp32c6::seams::TEST_ECHO_MARK;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image, skip_notice};

const DONE: &str = "[SEAM-ABI] done";

#[test]
#[ignore = "needs the test_seam_abi harness image; `just test-emu-c6`"]
fn the_harness_prints_silicons_answers_with_the_seams_off() {
    let Some(mut m) = harness("seams off", SeamRequest::none()) else {
        return;
    };
    let text = run_to_done(&mut m);
    assert!(has_line(&text, "[SEAM-ABI] echo=7"), "{text}");
    assert!(text.contains("[SEAM-ABI] take engaged=false"), "{text}");
    assert_eq!(m.seams().calls, 0);
}

#[test]
#[ignore = "needs the test_seam_abi harness image; `just test-emu-c6`"]
fn the_harness_prints_the_emulators_answers_with_the_seams_on() {
    let Some(mut m) = harness(
        "seams on",
        SeamRequest::strict("test=echo+test=take").unwrap(),
    ) else {
        return;
    };
    // The image resolves its seams once the hart first runs from the flash
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
        "the emulator's answer reached the caller through a0:\n{text}"
    );
    assert!(text.contains("[SEAM-ABI] take engaged=true"), "{text}");
    assert!(
        has_line(&text, "[SEAM-ABI] take 4 B: de ad be ef"),
        "the take filled the call's buffer:\n{text}"
    );
    assert!(
        m.seams().calls >= 3,
        "echo, a take with bytes, an empty take"
    );
    println!("{}", m.configuration_label());
}

fn harness(what: &str, request: SeamRequest) -> Option<Esp32C6Machine> {
    let elf = match fw_esp32c6_image(&FwImage::TEST_SEAM_ABI) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice(&format!("seam_abi_harness ({what})"), &reason);
            return None;
        }
    };
    Some(
        Esp32C6Builder::new()
            .app(AppSource::Path(elf))
            .usb_host(UsbHost::Attached { draining: true })
            .seams(request)
            .build()
            .expect("the harness image builds a machine"),
    )
}

fn has_line(text: &str, line: &str) -> bool {
    text.lines().any(|l| l.trim_end() == line)
}

fn run_to_done(m: &mut Esp32C6Machine) -> String {
    m.run_until(&StopCondition::after_micros(2_000_000).exit_on(DONE));
    let text = m.usb_sj().text();
    assert!(text.contains(DONE), "the harness ran to its end:\n{text}");
    text
}
