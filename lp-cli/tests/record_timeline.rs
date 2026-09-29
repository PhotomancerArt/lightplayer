//! `lp-cli record timeline` over a real recording: the shipped binary on
//! `fixtures/record/emulated-c6-session.jsonl`, trimmed (by `seq`, lines
//! kept verbatim, with `fixtures/record/trim_session.py <rec> 165 141 37.4 37.6`)
//! from a `?record=` session against an emulated ESP32-C6 on `lp-cli emu
//! serve` (headless Chrome, 2026-09-27, wire proto 30: the USB link is an
//! lp-link, replies packed): connect and identify, push the PLAYFUL choker,
//! open it in the editor, then the cable pulled at ~37 s and put back at
//! ~44 s. Wire lines are kept from the link's first chunk through the access
//! requests' answers (the host's stop before its `accessAdd`, which carries
//! the browser's key): a link's frames (and its learned table) decode only
//! from its start.

use std::path::Path;
use std::process::Command;

#[test]
fn a_real_recording_reads_as_a_timeline() {
    let out = timeline(&[]);
    let lines: Vec<&str> = out.lines().collect();

    assert!(
        lines[0].starts_with("  +0.000s  SESSION  4cd98d52a7be79e7 · "),
        "{out}"
    );
    for expected in [
        // Both ends open a link session (the handshake's two SYNs)…
        "  +0.124s  WIRE  →  serial:1  ~ link session 0xf8d6c9a1 (a reboot, reload or reconnect)",
        "  +0.126s  WIRE  ←  serial:1  ~ link session 0x0d83b3e2 (a reboot, reload or reconnect)",
        // …the board's log lines ride the link's log channel…
        "  +0.130s  WIRE  ←  serial:1  | [WARN] fw_esp32_common::lp_fs: [FS] Mount failed \
         (filesystem corrupt), formatting partition...",
        // …its hello is the first message of the session, and the page opts in…
        "  +0.132s  WIRE  ←  serial:1  hello id=0 579 B",
        "  +0.133s  WIRE  →  serial:1  setEncoding id=9007199254740991 81 B",
        // …after which replies come as learned JSON Pack payloads, decoded.
        "  +0.145s  WIRE  ←  serial:1  hello id=1 489 B packed",
        "  +0.161s  WIRE  ←  serial:1  accessList id=1073741824 61 B packed",
        "  +0.159s  REQ      c2#1073741824 access.list sent",
        "  +0.180s  REQ      c2#1073741824 access.list answered in 21.0 ms",
        " +18.160s  ROUTE    /device/mac:a0:f2:62:87:b4:8c → \
         /p/playful-choker-prjb64m0s0r1sya5ke3?on=mac:a0:f2:62:87:b4:8c   (slug-heal)",
        // The cable pull: the read in flight fails at once, and Studio goes
        // back to Devices.
        " +37.442s  CMD      DeviceHotplug/Disconnected",
        " +37.489s  REQ      c10#4311744602 project.read failed in 0.0 ms: transport error: \
         Transport error: The emulated board was detached.",
        " +43.729s  CMD      DeviceHotplug/Connected",
    ] {
        assert!(lines.contains(&expected), "missing {expected:?} in:\n{out}");
    }
    assert!(!out.contains("undecodable"), "{out}");
    assert!(!out.contains("unreadable"), "{out}");
}

#[test]
fn kinds_since_and_raw_narrow_the_same_recording() {
    let out = timeline(&["--kinds", "route,error", "--since", "37"]);
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            " +37.490s  ERROR    [warn/studio:lpa_client::pull_loop] project read id=4311744602: \
             transport error after 0 frames: Transport error: The emulated board was detached.",
            " +37.493s  ERROR    [warn/studio] the board under the editor went away; the editor \
             is closed",
            " +37.511s  ROUTE    /p/playful-choker-prjb64m0s0r1sya5ke3?on=mac:a0:f2:62:87:b4:8c \
             → /devices   (open-ended: home view shown, no open in flight, open stage idle, no \
             mismatch, saw_opening, no route open pending)",
        ]
    );

    let raw = timeline(&["--kinds", "wire", "--wire", "raw"]);
    // The host's first chunk is a link frame: `0x00`-delimited, COBS inside.
    assert!(
        raw.contains("  +0.124s  WIRE  →  serial:1  23 B  00 02 03 01 01 05 a1 c9 d6 f8"),
        "{raw}"
    );
}

fn timeline(extra: &[&str]) -> String {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/record/emulated-c6-session.jsonl");
    let output = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
        .args(["record", "timeline"])
        .arg(&fixture)
        .args(extra)
        .output()
        .expect("run lp-cli");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8")
}
