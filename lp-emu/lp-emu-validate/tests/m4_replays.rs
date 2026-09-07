//! M4's gates, as replays of committed transcripts.
//!
//! No firmware, no board, no emulator run: these read what the runner
//! recorded and check the claims the milestone is allowed to make. The
//! machine's own tests (`lp-emu/esp/lp-emu-esp32c6/tests/`) run the image
//! and are `#[ignore]`d for it; these are what makes the gate outlive the
//! sitting.
//!
//! ```bash
//! cargo run -p lp-cli -- validate record emu-m4 --config lp-emu:esp32c6:t1 \
//!   --date 2026-09-07 --commit d6cfaa2051ae --dirty --timeout-secs 20 \
//!   --image boot-idle-flash=target/emu-ref/d6cfaa205-boot-idle/fw-esp32c6 \
//!   --image upload-walk=target/emu-ref/d6cfaa205-boot-idle/fw-esp32c6
//! ```
//!
//! **Never edit a transcript.** A failure here is a regression or a
//! re-capture with its own header, never a digit changed in a `.txt`.

use std::path::PathBuf;

use lp_emu_validate::payload::SeriesSpec;
use lp_emu_validate::transcript::Transcript;
use lp_emu_validate::{Payload, find_payload};

const OURS: &str = "lp-emu-esp32c6-t1-2026-09-07-d6cfaa205.txt";

fn load(payload: &str) -> (&'static Payload, Transcript) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../transcripts/esp32c6")
        .join(payload)
        .join(OURS);
    let t = Transcript::load(&path).unwrap_or_else(|e| panic!("loading {}: {e:#}", path.display()));
    (find_payload(payload).unwrap(), t)
}

fn series(payload: &Payload, name: &str) -> &'static SeriesSpec {
    payload
        .series
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("series `{name}`"))
}

/// **Gate 1.** The flash-backed shipped image — the one that at the end of
/// M3 stopped at `SPIN SPI1+0x000 cmd` at 11 ms of emulated time — mounts
/// `lpfs`, formats it, and reaches the same idle heartbeat the memfs variant
/// does, with the spike report §5.1 figures.
///
/// Those figures are esp-emu's, on the same image bytes. All four heap
/// numbers and both stack numbers are byte-equal here, `largestFreeBlock`
/// included — which §11.2 records as the one heap figure that usually does
/// not transfer.
#[test]
fn the_flash_backed_boot_matches_spike_5_1() {
    let (payload, t) = load("boot-idle-flash");
    assert!(
        t.sentinel_line().is_some(),
        "the run reached the stack heartbeat"
    );

    // The `[FS]` pair `boot-idle` (memfs) cannot produce: this image mounts
    // a real partition through SPI1 and the mask ROM.
    let fs = t.series(series(payload, "fs-mount"));
    assert_eq!(fs.len(), 2, "the mount and the format: {fs:?}");
    let failed = fs.iter().find(|r| r.key == "Mount failed").expect("row");
    assert_eq!(
        failed.values["detail"], " (filesystem corrupt), formatting partition...",
        "an erased chip has no filesystem, and the firmware says which"
    );
    let formatted = fs
        .iter()
        .find(|r| r.key == "Formatted and mounted")
        .expect("row");
    assert_eq!(formatted.values["detail"], " fresh filesystem");

    let hello = t.series(series(payload, "hello"));
    assert_eq!(hello.len(), 1);
    assert_eq!(hello[0].values["proto"], "20");
    assert_eq!(hello[0].values["board_id"], "seeed/xiao-esp32-c6");

    let beat = t.series(series(payload, "heartbeat"));
    assert_eq!(beat.len(), 1, "the sentinel stops at the first one");
    let m = &beat[0].values;
    // Spike report §5.1, verbatim.
    assert_eq!(m["free_bytes"], "265392");
    assert_eq!(m["used_bytes"], "60144");
    assert_eq!(m["total_bytes"], "325536");
    assert_eq!(m["largest_free_block"], "199173");
    assert_eq!(m["reset_reason"], "power-on");
    assert_eq!(m["boot_count"], "1");
    assert_eq!(m["loaded_projects"], "", "nothing on the chip yet");

    let stack = t.series(series(payload, "stack-heartbeat"));
    assert_eq!(stack.len(), 1);
    // §5.1: `[stack] heartbeat: high-water 11844 B of 71328 B`. The stack is
    // 632 B smaller than the memfs variant's 71,960 — that is the
    // filesystem's `.bss`, and it is why the two images' high-water marks
    // are not comparable to each other, only each to its own reference.
    assert_eq!(stack[0].key, "71328");
    assert_eq!(stack[0].values["high_water"], "11844");
    assert_eq!(stack[0].values["headroom"], "59484");
}

/// **Gate 2.** The project-upload walk: the thirteen frames `lp-cli upload
/// examples/basic` sends, replayed from the committed host script.
///
/// The two figures the spike report §5.3 gives for esp-emu's run of the same
/// walk on the same image are byte-equal here: the load gate's ledger, and
/// the compiler's outputs.
#[test]
fn the_upload_walk_matches_spike_5_3() {
    let (payload, t) = load("upload-walk");
    assert!(t.sentinel_line().is_some(), "the shader compiled");

    // Every file the upload wrote, and the device's answer to each.
    let writes = t.series(series(payload, "fs-write"));
    let mut paths: Vec<&str> = writes.iter().map(|r| r.key.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        vec![
            "/projects/Basic/clock.json",
            "/projects/Basic/fixture.json",
            "/projects/Basic/fixture.map2d.json",
            "/projects/Basic/module.json",
            "/projects/Basic/output.json",
            "/projects/Basic/project.json",
            "/projects/Basic/shader.glsl",
            "/projects/Basic/shader.json",
        ],
    );
    for row in &writes {
        assert_eq!(row.values["error"], "null", "{} was refused", row.key);
    }
    // `shader.glsl` is 4,365 bytes and arrives as two `writeChunk`s; the
    // series is keyed on the path, so its row is the last of them.
    assert_eq!(
        writes
            .iter()
            .find(|r| r.key.ends_with("shader.glsl"))
            .unwrap()
            .values["op"],
        "writeChunk"
    );

    // The heap gates around the load. §5.3's esp-emu figures, verbatim.
    let gates = t.series(series(payload, "load-gate"));
    let gate = |name: &str| {
        &gates
            .iter()
            .find(|r| r.key == name)
            .unwrap_or_else(|| panic!("gate `{name}` in {gates:?}"))
            .values
    };
    assert_eq!(gate("load_project after")["free_bytes"], "220532");
    assert_eq!(gate("load_project after")["used_bytes"], "105004");
    // The other three gates, as measured on this configuration. `stop_all_
    // projects before` is 264,712 here against §5.3's 264,716 — 4 B, the
    // same 4 B §11.2 records between esp-emu and silicon heartbeats on one
    // image, and the walk's own host-connect drift (§7) is 64 B, so this is
    // inside the noise the report already documents.
    assert_eq!(gate("stop_all_projects before")["free_bytes"], "264712");
    assert_eq!(gate("load_project before")["free_bytes"], "258284");

    // The compiler's outputs: the same source produces the same numbers
    // wherever it compiles, and §5.3 has these.
    let compile = t.series(series(payload, "shader-compile"));
    assert_eq!(compile.len(), 1);
    let c = &compile[0].values;
    assert_eq!(c["lpir_inst_count"], "573");
    assert_eq!(c["lpir_func_count"], "12");
    assert_eq!(c["lpir_import_count"], "7");
    assert_eq!(c["final_inst_count"], "2048");
    assert_eq!(c["final_code_size"], "8192");
    assert_eq!(c["float_mode"], "fixed");
    // `elapsed_ms` is graded Timing and is deliberately not asserted: it
    // reads 51 ms here and 52 ms under the live client on the same image.
    assert!(c.contains_key("elapsed_ms"));

    // The filesystem the walk wrote into was formatted by this same boot —
    // the walk starts from an erased chip, as the desk walk did.
    let fs = t.series(series(payload, "fs-mount"));
    assert_eq!(fs.len(), 2, "{fs:?}");
}

/// The walk stops where M5 begins, and the transcript says so rather than
/// leaving a reader to wonder why there is no `projectRead` answer.
///
/// `Ws281xOutput::write` waits for the **RMT interrupt**; RMT is an
/// accept-and-remember register file, which raises no interrupt source at
/// all; so the driver spins to its 50 ms deadline and `LpServer::tick`
/// returns a project tick error before it answers request 12. Everything up
/// to and including `loadProject` lands.
#[test]
fn the_walk_ends_at_the_rmt_frame_which_is_m5s() {
    let (_, t) = load("upload-walk");
    let body = t.lines.join("\n");
    assert!(
        body.contains(r#""loadProject":{"handle":1}"#),
        "the project loaded"
    );
    assert!(
        body.contains("RMT channel 0 frame did not complete within 50 ms"),
        "the RMT gap should be visible in the transcript, not silent"
    );
    assert!(
        !body.contains(r#"M!{"id":12,"#),
        "request 12 was answered — has M5's RMT model landed? Update this test \
         and the walk's sentinel."
    );
}

/// The negative control. One wrong digit in a heap figure has to be visible,
/// or none of the above means anything.
#[test]
fn a_corrupted_load_gate_digit_is_a_different_series_value() {
    let (payload, t) = load("upload-walk");
    let real = t.series(series(payload, "load-gate"));
    let after = real
        .iter()
        .find(|r| r.key == "load_project after")
        .expect("the load gate");
    assert_eq!(after.values["free_bytes"], "220532");

    // The file on disk is never touched: this reloads the body with one
    // digit changed, the way `m3_replays.rs` does.
    let header = t.header.clone();
    let body = t.lines.join("\n").replace("220532 B free", "220533 B free");
    let corrupted = Transcript::from_parts(header, &body).expect("parses");
    let corrupted = corrupted.series(series(payload, "load-gate"));
    let after = corrupted
        .iter()
        .find(|r| r.key == "load_project after")
        .expect("the load gate");
    assert_eq!(
        after.values["free_bytes"], "220533",
        "a changed digit must reach the series, or the gate is decorative"
    );
}
