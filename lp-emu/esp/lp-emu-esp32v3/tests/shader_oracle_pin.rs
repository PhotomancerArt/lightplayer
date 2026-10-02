//! `walks/shader-oracle.script` and the oracle constant. **M4 P4's gate** —
//! the first lit frame the product firmware puts on the classic's IO18 is the
//! host oracle's frame, read three ways: the firmware's own `[OUT] dump`, the
//! pad, and `[ORACLE]` — used to run here, on the `frame-dump` image, over
//! the committed `M!` walk.
//!
//! ⚠️ Since wire proto 32 (plan `classic-uart-on-lp-link`) the shipped image
//! speaks lp-link on UART0 and no longer reads `M!` lines, the `[OUT] dump`
//! is a log record on the link's log channel, and nothing under `lp-emu/`
//! may host a link (the MIT fence). The gate therefore moved, with every
//! assertion — the three readings, every later frame the same frame, the
//! routing and colour order, the run reaching its deadline with nothing
//! unmapped, and two runs writing byte-identical `--dump-frames` files — to
//! `lp-cli/tests/emu_v3_link_gates.rs`'s
//! `the_frame_dump_walk_reads_the_oracles_frame_three_ways`, which deploys
//! the same retargeted project over the product's own link host and reads the
//! same pad with the same decoder against the same constant. The console
//! repair this file carried (`deinterleave`, for the PR #300 interleaving
//! defect) did not move: on the link every record arrives whole. The
//! walk-side twin is `just walk-esp32v3-emu`.
//!
//! What stays here is what needs no firmware: the committed script's shape
//! (the pre-lp-link record of the walk), the oracle constant's own
//! transcription check, and the FNV-1a the constant is checked with.

use std::path::{Path, PathBuf};

use lp_emu_esp32v3::control::parse_byte_script;

/// `cargo test -p lpa-server --test shader_oracle_frame -- --nocapture`, run
/// **in this worktree** on branch `claude/xt-m4-p4-frame-three-ways` after
/// `just ci-prereqs`, 2026-09-11:
///
/// ```text
/// [ORACLE] leds=64 shown=64 crc=0x55772254 lit=64
/// [ORACLE] rgb=324a0208376a1c28…4c2d05
/// [ORACLE-RV32] leds=64 shown=64 crc=0x55772254 lit=64
/// [ORACLE-RV32] rgb=324a0208376a1c28…4c2d05
/// [ORACLE-DIFF] wasmtime vs rv32-emu: 0 differing bytes of 192
/// ```
///
/// The two engines agreed on every byte, so one constant stands for both.
/// These are the **classic's own** oracle numbers: the project and the
/// engines are the same as the C6's, so the 384 characters are the same 384
/// characters — but that was checked here rather than copied, because a
/// copied constant that happens to be right teaches nothing when it is
/// wrong.
const ORACLE_RGB: &str = "324a0208376a1c2889007668098b4b0375544602631253162b0f7051068a838b000097890b63b208a1601b30951b1c72660069af481900a49554e3212b48e41955cdad4d154f047b103e90441ec10ed47200bcb627f657019fb523c13e3794161c952e04a8743b36e681e90e225ef47f09d1174ebc035c8009447f3fb11b6112ca048dc419dd5fae02903ab21f015c6f026047006a750aa45b69b20834c32b7e8f0012913c086a360144365600567c064d430e9127239632148702475f4c2d05";
/// The oracle's own `crc=`, over the same 192 bytes.
const ORACLE_CRC: u32 = 0x5577_2254;

/// The oracle project's 64 LEDs.
const LEDS: usize = 64;

fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("walks")
        .join("shader-oracle.script")
}

/// FNV-1a, 32-bit — the oracle's `crc=` (`shader_oracle_frame::fnv1a`) and
/// the firmware's own `frame_checksum`, restated inside the `lp-emu/` fence
/// rather than imported across it (`just lint-emu-fence`).
fn fnv1a(data: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in data {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// The committed script is what it claims to be: twelve requests, the hello
/// first, `projectRead` last. No firmware, so this runs everywhere.
///
/// ⚠️ M4 **P4b** landed this same file at the same path (#711), and this
/// branch kept that copy on the rebase rather than its own. Regenerating it
/// here would undo that.
#[test]
fn the_walk_script_is_the_twelve_requests_the_client_sends() {
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
        read.starts_with("after \"\\\"id\\\":10,\""),
        "projectRead must wait for the loadProject ANSWER, not fire on a timer: {read}"
    );
    // The endpoint the walk uploads is the board's own, never the project's
    // `D10` — an endpoint this board does not have never opens.
    assert!(
        text.contains("ws281x:local:IO18"),
        "the walk must upload the IO18-retargeted scratch copy"
    );
    assert!(parse_byte_script(&text).is_ok());
}

/// The published FNV-1a 32-bit vectors. If this fails, the copy in this file
/// disagrees with the firmware's `frame_checksum` and the oracle's `crc=` —
/// and the gates above would be comparing against a different function while
/// looking healthy.
#[test]
fn fnv1a_matches_the_published_vectors() {
    assert_eq!(fnv1a(b""), 0x811c_9dc5);
    assert_eq!(fnv1a(b"a"), 0xe40c_292c);
    assert_eq!(fnv1a(b"foobar"), 0xbf9c_f968);
}

/// The oracle constant is 64 LEDs of RGB and its own checksum — a
/// transcription check, so a truncated paste fails here rather than as a
/// frame mismatch.
#[test]
fn the_oracle_constant_is_sixty_four_pixels_and_its_own_crc() {
    assert_eq!(ORACLE_RGB.len(), LEDS * 3 * 2, "384 hex characters");
    let bytes: Vec<u8> = (0..ORACLE_RGB.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&ORACLE_RGB[i..i + 2], 16).expect("hex"))
        .collect();
    assert_eq!(fnv1a(&bytes), ORACLE_CRC);
}
