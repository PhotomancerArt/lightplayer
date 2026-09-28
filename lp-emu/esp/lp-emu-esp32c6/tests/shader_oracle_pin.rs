//! `walks/shader-oracle.script`: the committed `M!` walk that uploaded
//! `projects/test/shader-oracle` to the shipped image for M5 P4's pin claim
//! (G4-1: the first lit frame off gpio18 is the host oracle's frame).
//!
//! ⚠️ Since wire proto 30 (plan `lp-link-usb-cutover`) the shipped image
//! speaks lp-link on USB and no longer reads `M!` lines, and nothing under
//! `lp-emu/` may host a link (the MIT fence). G4-1 itself — under t1 and t2,
//! and two runs dumping identical frames — therefore moved, unchanged in
//! what it proves, to `lp-cli/tests/emu_usb_link_gates.rs`, which deploys
//! the same project over the product's own link host and reads the same pad
//! with the same decoder against the same oracle constant.
//!
//! What stays here is the script's own shape check, because the script is
//! still committed: it is a validation payload's host script
//! (`lp-emu-validate`) and the S3's `pin_frames` holds its own copy against
//! it. No firmware.

use lp_emu_esp32c6::control::parse_byte_script;

fn script_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("walks")
        .join("shader-oracle.script")
}

/// The committed script is what it claims to be: twelve requests, the
/// hello first, `projectRead` last and waiting on the compile line — the
/// same check `upload_walk.rs` makes of `examples-basic.script`. No firmware.
#[test]
fn the_walk_script_is_the_twelve_frames_the_client_sends() {
    let text = std::fs::read_to_string(script_path()).expect("committed");
    let afters = text.lines().filter(|l| l.starts_with("after ")).count();
    assert_eq!(afters, 12, "one `after` per request");
    for id in [
        "18446744073709551615",
        "1",
        "2",
        "3",
        "4",
        "5",
        "6",
        "7",
        "8",
        "9",
        "10",
        "11",
    ] {
        assert!(
            text.contains(&format!("M!{{\\\"id\\\":{id},")),
            "request {id} is missing from the script"
        );
    }
    let first = text
        .lines()
        .find(|l| l.starts_with("after "))
        .expect("a first step");
    assert!(
        first.contains("[RECOVERY] boot complete (first frame served)"),
        "{first}"
    );
    let read = text
        .lines()
        .find(|l| l.contains("\\\"projectRead\\\""))
        .expect("the projectRead request");
    assert!(
        read.starts_with("after \"[shader-node] compilation succeeded\""),
        "projectRead must wait for the compile, not for loadProject's acknowledgement: {read}"
    );
    assert!(parse_byte_script(&text).is_ok());
}
