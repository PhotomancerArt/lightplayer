//! M6 P5: the same upload walk, over the link the product ships.
//!
//! `upload_walk.rs` runs `walks/examples-basic.script` against the spike's
//! UART0 link, because that is the link an emulator had when M4 wrote it.
//! This runs the **same script file** against the modelled USB-Serial-JTAG
//! block with a host attached and draining, on the **same commit built with
//! no cherry-pick** — the bytes a silicon flash of `d6cfaa205` puts on a
//! board. Silicon's own §11.3 walk went over USB-Serial-JTAG through
//! `usb-tcp-bridge.py`, so this is the like-for-like run and M4's was the
//! proxy.
//!
//! What it pins:
//!
//! 1. The walk lands over the USB link: every request answered in order,
//!    nothing lost on the wire, `loadProject` returning handle 1.
//! 2. `[mem] load_project after: 220532 B free / 105004 B used` and the
//!    compiler's outputs — **byte-equal to the UART0 run and to spike report
//!    §5.3**. The link driver is not in the heap ledger, and this is what
//!    says so.
//! 3. The flash census is the same census. A different link must not move
//!    one page program.
//! 4. **The project survives the machine, over this link too**: a second
//!    machine on the same flash file mounts what the first formatted, finds
//!    one entry, auto-loads `/projects/Basic`, and writes nothing.
//! 5. Two runs are byte-identical — the socket is a file here, and the
//!    script's waits resolve in guest cycles either way.
//!
//! `#[ignore]`d for the usual reason (`test_support`).

use std::path::{Path, PathBuf};

use lp_emu_esp32c6::control::parse_byte_script;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, TimeGrade, UsbHost,
};
use lp_emu_esp32c6::test_support::{ReferenceImage, reference_image, skip_notice};

/// Enough emulated time for the whole upload and the load.
const WALK_US: u64 = 20_000_000;
const COMPILE_SENTINEL: &str = "[shader-node] compilation succeeded";
/// §5.3, verbatim — and M4's UART0 run's, verbatim.
const LOAD_AFTER: &str = "[mem] load_project after: 220532 B free / 105004 B used (215k / 102k)";
const COMPILE_OUTPUTS: &str = "lpir_inst_count=573, lpir_func_count=12, lpir_import_count=7, \
     final_inst_count=2048, final_code_size=8192 bytes, float=fixed)";

fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("walks")
        .join("examples-basic.script")
}

struct Walk {
    m: Esp32C6Machine,
    outcome: Outcome,
    text: String,
}

/// The walk on the USB link. Unlike `upload_walk.rs` this calls the crate's
/// own parser rather than copying the grammar: the point of P5's change is
/// that ONE grammar serves both links, and a private copy here would let the
/// two drift apart silently — which is exactly the bug the shared parser
/// exists to prevent.
fn run_walk(elf: &Path, backing: FlashBacking) -> Walk {
    let text = std::fs::read_to_string(script_path()).expect("the walk script is committed");
    let script = parse_byte_script(&text).expect("the committed walk parses");
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .usb_host(UsbHost::Attached { draining: true })
        .usb_script_source(script)
        .flash(backing)
        .strict(true)
        .time_grade(TimeGrade::T1)
        .build()
        .expect("the reference image builds a machine");
    let outcome = m.run_until(&StopCondition::after_micros(WALK_US).exit_on(COMPILE_SENTINEL));
    m.flush_flash().expect("the flash image writes back");
    let text = String::from_utf8_lossy(&m.usb_sj().bytes()).into_owned();
    Walk { m, outcome, text }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lp-emu-m6-p5-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.join("flash.bin")
}

#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-c6`"]
fn the_walk_over_the_usb_link_lands_the_same_project_with_the_same_figures() {
    let elf = match reference_image(&ReferenceImage::SHIPPED_USB) {
        Ok(path) => path,
        Err(reason) => return skip_notice("upload_walk_usb", &reason),
    };
    let path = scratch("upload");
    let _ = std::fs::remove_file(&path);
    let walk = run_walk(&elf, FlashBacking::File(path.clone()));

    assert!(
        matches!(walk.outcome, Outcome::ExitMatched { .. }),
        "the walk never reached the shader compile: {:?}\n{}",
        walk.outcome,
        walk.text
    );
    assert_eq!(
        walk.m.bus.unmapped_reads() + walk.m.bus.unmapped_writes(),
        0,
        "unmapped"
    );
    // A dropped byte reads as a corrupt frame three layers up. The OUT
    // endpoint is a different path from UART0's RX FIFO, so this is a real
    // question again and not an inherited answer.
    assert!(
        !walk.text.contains("dropping unparseable"),
        "the guest lost bytes:\n{}",
        walk.text
    );

    // Requests 1..=11 answered in order, once each; 12 is `projectRead`.
    let mut from = 0;
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
        let needle = format!("M!{{\"id\":{id},");
        let at = walk.text[from..]
            .find(&needle)
            .unwrap_or_else(|| panic!("no answer to request {id} after byte {from}"));
        from += at + needle.len();
    }
    assert!(
        walk.text.contains("\"loadProject\":{\"handle\":1}"),
        "{}",
        walk.text
    );

    // The figures. Byte-equal to the UART0 run of the same script — which is
    // the claim: two link drivers, one heap ledger.
    assert!(walk.text.contains(LOAD_AFTER), "{}", walk.text);
    assert!(walk.text.contains(COMPILE_OUTPUTS), "{}", walk.text);

    // §8's esp-emu breakdown, unmoved by the link.
    let census = walk.m.flash_census();
    assert_eq!(census.reads, 13_528, "{census}");
    assert_eq!(census.programs, 634, "{census}");
    assert_eq!(census.sector_erases, 29, "{census}");
    assert_eq!(census.block_erases, 0, "{census}");
    assert_eq!(census.write_enables, 664, "{census}");

    // The project survives the machine, over this link too: a second boot on
    // the same file mounts what the first formatted and loads it.
    let mut second = Esp32C6Builder::new()
        .app(AppSource::Path(elf.clone()))
        .usb_host(UsbHost::Attached { draining: true })
        .flash(FlashBacking::Copy(path.clone()))
        .strict(true)
        .time_grade(TimeGrade::T1)
        .build()
        .expect("a machine on the walked chip");
    let outcome =
        second.run_until(&StopCondition::after_micros(6_000_000).exit_on("boot complete"));
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "{outcome:?}"
    );
    let boot = String::from_utf8_lossy(&second.usb_sj().bytes()).into_owned();
    assert!(
        !boot.contains("[FS]"),
        "the second boot reformatted:\n{boot}"
    );
    assert!(
        boot.contains("Boot: found configured startup project: Basic")
            && boot.contains("Boot: auto-loaded project /projects/Basic"),
        "{boot}"
    );
    let reboot = second.flash_census();
    assert_eq!(
        (reboot.programs, reboot.sector_erases, reboot.write_enables),
        (0, 0, 0),
        "a mount-and-load writes nothing: {reboot}"
    );

    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-c6`"]
fn two_scripted_usb_walks_are_byte_identical() {
    let elf = match reference_image(&ReferenceImage::SHIPPED_USB) {
        Ok(path) => path,
        Err(reason) => return skip_notice("upload_walk_usb", &reason),
    };
    let a = run_walk(&elf, FlashBacking::Blank);
    let b = run_walk(&elf, FlashBacking::Blank);
    assert_eq!(a.outcome, b.outcome);
    assert_eq!(a.m.cycles(), b.m.cycles());
    assert_eq!(a.m.instructions(), b.m.instructions());
    assert_eq!(a.m.flash_census(), b.m.flash_census());
    assert_eq!(a.text, b.text, "two runs of the same script diverged");
}
