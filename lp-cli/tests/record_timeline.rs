//! `lp-cli record timeline` over a real recording: the shipped binary on
//! `fixtures/record/emulated-c6-session.jsonl`, trimmed (by `seq`, lines
//! kept verbatim) from a `?record=` session against an emulated ESP32-C6
//! (`just studio-dev-emu`, headless Chrome, 2026-09-26, wire proto 28 with
//! the learned dictionary): connect and identify, push and open the PLAYFUL
//! choker, home and back (which reopens it), Play, back to Devices. Wire
//! lines are kept only from the link's start through its first heartbeat: a
//! learned-dictionary stream decodes only from its beginning, so a later
//! chunk without the ones before it would not read.

use std::path::Path;
use std::process::Command;

#[test]
fn a_real_recording_reads_as_a_timeline() {
    let out = timeline(&[]);
    let lines: Vec<&str> = out.lines().collect();

    assert!(
        lines[0].starts_with("  +0.000s  SESSION  cddf7c3ee003561a · "),
        "{out}"
    );
    for expected in [
        // The page's hello, over its Web Serial write…
        "  +5.193s  WIRE  →  serial:1  hello id=1 25 B",
        // …the board's boot text, torn across chunks and put back together…
        "  +5.257s  WIRE  ←  serial:1  | [INIT] Initializing board...",
        // …and, once it packs its replies, a learned JSON Pack frame decoded.
        "  +5.279s  WIRE  ←  serial:1  accessList id=1073741824 71 B packed",
        "  +5.264s  REQ      c11#1073741824 access.list sent",
        "  +5.286s  REQ      c11#1073741824 access.list answered in 22.0 ms",
        " +15.506s  ROUTE    /device/mac:a0:f2:62:87:b4:8c → \
         /p/playful-choker-prjk73kh54gdkdrj608?on=mac:a0:f2:62:87:b4:8c   (slug-heal)",
        " +26.527s  OPEN     on-device:uploading",
        " +40.599s  TOAST    [info] Closed XIAO ESP32-C6 · Sep 26 — the board keeps running",
        " +40.746s  REQ      c15#1090519058 project.read failed in 64.0 ms: \
         server error: Project not found: handle 1",
    ] {
        assert!(lines.contains(&expected), "missing {expected:?} in:\n{out}");
    }
    assert!(!out.contains("undecodable"), "{out}");
}

#[test]
fn kinds_since_and_raw_narrow_the_same_recording() {
    let out = timeline(&["--kinds", "route,error", "--since", "40"]);
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            " +40.573s  ROUTE    /p/playful-choker-prjk73kh54gdkdrj608/play?on=mac:a0:f2:62:87:b4:8c \
             → /p/playful-choker-prjk73kh54gdkdrj608?on=mac:a0:f2:62:87:b4:8c   (browser-nav)",
            " +40.615s  ROUTE    /p/playful-choker-prjk73kh54gdkdrj608?on=mac:a0:f2:62:87:b4:8c \
             → /devices   (browser-nav)",
            " +40.746s  ERROR    [warn/studio:lpa_client::pull_loop] project read id=1090519058: \
             stream error after 1 frames: Server(\"Project not found: handle 1\")",
        ]
    );

    let raw = timeline(&["--kinds", "wire", "--wire", "raw"]);
    assert!(
        raw.contains("  +5.193s  WIRE  →  serial:1  25 B  \"M!{\\\"id\\\":1,"),
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
