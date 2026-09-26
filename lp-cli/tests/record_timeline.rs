//! `lp-cli record timeline` over a real recording: the shipped binary on
//! `fixtures/record/emulated-c6-session.jsonl`, trimmed (by `seq`, lines
//! kept verbatim) from a `?record=` session against an emulated ESP32-C6
//! (`just studio-dev-emu`, headless Chrome, 2026-09-26): connect and
//! identify, push and open the PLAYFUL choker, Play, back to Devices.

use std::path::Path;
use std::process::Command;

#[test]
fn a_real_recording_reads_as_a_timeline() {
    let out = timeline(&[]);
    let lines: Vec<&str> = out.lines().collect();

    assert!(
        lines[0].starts_with("  +0.000s  SESSION  cb2049916715f0a2 · "),
        "{out}"
    );
    for expected in [
        // The board's own hello, over the page's Web Serial write…
        " +17.632s  WIRE  →  serial:1  hello id=1 25 B",
        // …its boot text, torn across chunks and put back together…
        " +17.651s  WIRE  ←  serial:1  | [INIT] Initializing board...",
        // …and, once it packs its replies, a JSON Pack frame decoded.
        " +17.672s  WIRE  ←  serial:1  accessList id=1073741824 28 B packed",
        " +17.657s  REQ      c11#1073741824 access.list sent",
        " +17.680s  REQ      c11#1073741824 access.list answered in 23.0 ms",
        "+343.870s  ROUTE    /device/mac:a0:f2:62:87:b4:8c → \
         /p/playful-choker-prj1zq7spg01322azmq?on=mac:a0:f2:62:87:b4:8c   (slug-heal)",
        "+364.168s  OPEN     on-device:uploading",
        "+404.727s  TOAST    [info] Closed XIAO ESP32-C6 · Sep 26 — the board keeps running",
        "+404.906s  WIRE  ←  serial:1  error.error id=1090520149 48 B packed",
        "+404.926s  REQ      c15#1090520149 project.read failed in 85.0 ms: \
         server error: Project not found: handle 1",
    ] {
        assert!(lines.contains(&expected), "missing {expected:?} in:\n{out}");
    }
    assert!(!out.contains("undecodable"), "{out}");
}

#[test]
fn kinds_since_and_raw_narrow_the_same_recording() {
    let out = timeline(&["--kinds", "route,error", "--since", "400"]);
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            "+404.692s  ROUTE    /p/playful-choker-prj1zq7spg01322azmq/play?on=mac:a0:f2:62:87:b4:8c \
             → /p/playful-choker-prj1zq7spg01322azmq?on=mac:a0:f2:62:87:b4:8c   (browser-nav)",
            "+404.743s  ROUTE    /p/playful-choker-prj1zq7spg01322azmq?on=mac:a0:f2:62:87:b4:8c \
             → /devices   (browser-nav)",
            "+404.926s  ERROR    [warn/studio:lpa_client::pull_loop] project read id=1090520149: \
             stream error after 1 frames: Server(\"Project not found: handle 1\")",
        ]
    );

    let raw = timeline(&["--kinds", "wire", "--wire", "raw"]);
    assert!(
        raw.contains(" +17.632s  WIRE  →  serial:1  25 B  \"M!{\\\"id\\\":1,"),
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
