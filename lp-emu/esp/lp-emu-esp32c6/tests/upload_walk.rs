//! M4: the project-upload walk, replayed from a script in guest time.
//!
//! `walks/examples-basic.script` is the host half of `lp-cli upload
//! examples/basic`, captured from the real client over a socket
//! (`scripts/emu/upload-walk.sh`) and turned into a `--uart0-script` by
//! `scripts/emu/walk-script.py`. See `walks/README.md`.
//!
//! What this pins:
//!
//! 1. All thirteen requests are answered, in order, once each.
//! 2. `[mem] load_project after: 220532 B free / 105004 B used` — **byte-equal
//!    to the spike report §5.3**, esp-emu's figure for the same walk on the
//!    same image.
//! 3. The JIT compile line is byte-equal too, outputs included.
//! 4. The flash command census is esp-emu's peripheral-inventory breakdown
//!    exactly: 13,528 reads, 634 page programs, 29 sector erases, 664
//!    write-enables.
//! 5. Two runs are byte-identical.
//! 6. The project survives a power cycle: a second machine on the same flash
//!    file mounts without formatting and auto-loads `/projects/Basic`.
//!
//! What it does **not** pin, and why: the `projectRead` answer (request 12).
//! `Ws281xOutput::write` waits for the RMT interrupt, an accept block raises
//! none, so the driver spins to its 50 ms deadline, `LpServer::tick` returns
//! a project tick error, and request 12 is never answered. That is M5's
//! channel model, not this milestone's — see `periph::accept::rmt`.
//!
//! `#[ignore]`d for the usual reason (`test_support`).

use std::path::{Path, PathBuf};

use lp_emu_esp_common::ScriptedSource;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, TimeGrade,
};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{ReferenceImage, reference_image, skip_notice, workspace_root};

/// Enough emulated time for the whole upload and the load. The walk itself
/// finishes in about 1.06 s of guest time.
const WALK_US: u64 = 20_000_000;

/// The sentinel: the shader compile, which is the last thing the load
/// produces. The load-gate line is a few lines earlier.
const COMPILE_SENTINEL: &str = "[shader-node] compilation succeeded";

/// §5.3, verbatim.
const LOAD_AFTER: &str = "[mem] load_project after: 220532 B free / 105004 B used (215k / 102k)";
/// The compiler's **outputs** from the same line, which §5.3 also has and
/// which are allocator-and-codegen facts. `elapsed=` is deliberately not
/// here: a time field is reported, never compared (plan PD9/D13), and this
/// one reads 51 ms under the script and 52 ms under the live client.
const COMPILE_OUTPUTS: &str = "lpir_inst_count=573, lpir_func_count=12, lpir_import_count=7, \
     final_inst_count=2048, final_code_size=8192 bytes, float=fixed)";

/// The thirteen request ids, in order. `18446744073709551615` is `u64::MAX`,
/// the readiness engine's hello.
const REQUEST_IDS: &[&str] = &[
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
    "12",
];

fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("walks")
        .join("examples-basic.script")
}

/// Parse the committed walk into a source. Deliberately a copy of the CLI's
/// grammar rather than a call into it: a test that used the binary's parser
/// would pass with a script the binary alone can read.
fn walk_script() -> ScriptedSource {
    let text = std::fs::read_to_string(script_path()).expect("the walk script is committed");
    let mut source = ScriptedSource::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (kind, rest) = line
            .split_once(' ')
            .unwrap_or_else(|| panic!("line {}: {line}", n + 1));
        match kind {
            "after" => {
                let (needle, rest) = take_quoted(rest.trim());
                let (delay, rest) = take_delay(rest.trim());
                let (bytes, _) = take_quoted(rest);
                source.push_after(needle, delay, bytes);
            }
            "then" => {
                let (delay, rest) = take_delay(rest.trim());
                let (bytes, _) = take_quoted(rest);
                source.push_then(delay, bytes);
            }
            ms => {
                let ms: u64 = ms.parse().unwrap_or_else(|_| panic!("line {}", n + 1));
                let (bytes, _) = take_quoted(rest.trim());
                source.push(ms * 1_000 * memmap::CYCLES_PER_US, bytes);
            }
        }
    }
    source
}

fn take_delay(text: &str) -> (u64, &str) {
    let Some(rest) = text.strip_prefix('+') else {
        return (0, text);
    };
    let (num, tail) = rest.split_once(' ').expect("`+<ms>` then bytes");
    let ms: u64 = num
        .trim_end_matches("ms")
        .parse()
        .expect("a millisecond count");
    (ms * 1_000 * memmap::CYCLES_PER_US, tail.trim())
}

fn take_quoted(text: &str) -> (Vec<u8>, &str) {
    let body = text.strip_prefix('"').expect("a quoted string");
    let mut out = Vec::with_capacity(body.len());
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return (out, &body[i + 1..]),
            '\\' => match chars.next().map(|(_, c)| c) {
                Some('n') => out.push(b'\n'),
                Some('r') => out.push(b'\r'),
                Some('t') => out.push(b'\t'),
                Some('0') => out.push(0),
                Some('\\') => out.push(b'\\'),
                Some('"') => out.push(b'"'),
                Some('x') => {
                    let hex: String = (&mut chars).take(2).map(|(_, c)| c).collect();
                    out.push(u8::from_str_radix(&hex, 16).expect("a hex byte"));
                }
                other => panic!("unknown escape {other:?}"),
            },
            other => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    panic!("unterminated string in the walk script");
}

struct Walk {
    m: Esp32C6Machine,
    outcome: Outcome,
    text: String,
}

fn run_walk(elf: &Path, backing: FlashBacking) -> Walk {
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .uart0_script(walk_script())
        .flash(backing)
        .strict(true)
        .time_grade(TimeGrade::T1)
        .build()
        .expect("the reference image builds a machine");
    let outcome = m.run_until(&StopCondition::after_micros(WALK_US).exit_on(COMPILE_SENTINEL));
    m.flush_flash().expect("the flash image writes back");
    let text = String::from_utf8_lossy(&m.uart0().bytes()).into_owned();
    Walk { m, outcome, text }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lp-emu-m4-walk-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.join("flash.bin")
}

#[test]
#[ignore = "needs the flash-backed reference image; run through `just test-emu-c6`"]
fn the_upload_walk_lands_the_project_with_the_spike_reports_own_figures() {
    let elf = match reference_image(&ReferenceImage::BOOT_IDLE) {
        Ok(path) => path,
        Err(reason) => return skip_notice("upload_walk", &reason),
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
    // Not one byte was lost on the wire: a dropped RX byte reads as a
    // corrupt frame three layers up, and that is the whole reason the walk
    // is paced.
    assert!(
        !walk.text.contains("dropping unparseable"),
        "the guest lost bytes:\n{}",
        walk.text
    );

    // Every request answered, in order, once each. Requests 1..=11; 12 is
    // `projectRead`, which waits for M5 (see the module docs).
    let mut from = 0;
    for id in &REQUEST_IDS[..12] {
        let needle = format!("M!{{\"id\":{id},");
        let at = walk.text[from..]
            .find(&needle)
            .unwrap_or_else(|| panic!("no answer to request {id} after byte {from}"));
        from += at + needle.len();
    }
    assert_eq!(
        walk.text.matches("M!{\"id\":12,").count(),
        0,
        "request 12 was answered — has M5's RMT model landed? Update this test."
    );
    assert!(
        walk.text.contains("\"loadProject\":{\"handle\":1}"),
        "{}",
        walk.text
    );

    // The two figures the walk exists to produce.
    assert!(walk.text.contains(LOAD_AFTER), "{}", walk.text);
    assert!(walk.text.contains(COMPILE_OUTPUTS), "{}", walk.text);

    // The flash traffic, against the spike report §8's esp-emu breakdown for
    // the same walk.
    let census = walk.m.flash_census();
    assert_eq!(census.reads, 13_528, "{census}");
    assert_eq!(census.programs, 634, "{census}");
    assert_eq!(census.sector_erases, 29, "{census}");
    assert_eq!(census.block_erases, 0, "{census}");
    assert_eq!(census.write_enables, 664, "{census}");

    // And the project is on the chip: a second machine on the same file
    // mounts it and auto-loads it, with no format and no write at all.
    let mut second = Esp32C6Builder::new()
        .app(AppSource::Path(elf.clone()))
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
    let boot = String::from_utf8_lossy(&second.uart0().bytes()).into_owned();
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
#[ignore = "needs the flash-backed reference image; run through `just test-emu-c6`"]
fn two_scripted_walks_are_byte_identical() {
    let elf = match reference_image(&ReferenceImage::BOOT_IDLE) {
        Ok(path) => path,
        Err(reason) => return skip_notice("upload_walk", &reason),
    };
    let a = run_walk(&elf, FlashBacking::Blank);
    let b = run_walk(&elf, FlashBacking::Blank);
    assert_eq!(a.outcome, b.outcome);
    assert_eq!(a.m.cycles(), b.m.cycles());
    assert_eq!(a.m.instructions(), b.m.instructions());
    assert_eq!(a.m.flash_census(), b.m.flash_census());
    assert_eq!(a.text, b.text, "two runs of the same script diverged");
}

#[test]
fn the_walk_script_is_the_thirteen_frames_the_client_sends() {
    // No firmware needed: the script is data, and this is the check that it
    // still is what it claims to be after a regeneration.
    let text = std::fs::read_to_string(script_path()).expect("committed");
    let afters = text.lines().filter(|l| l.starts_with("after ")).count();
    let thens = text.lines().filter(|l| l.starts_with("then ")).count();
    assert_eq!(afters, 13, "one `after` per request");
    assert!(thens > 0, "large requests are written in paced chunks");
    for id in REQUEST_IDS {
        assert!(
            text.contains(&format!("M!{{\\\"id\\\":{id},")),
            "request {id} is missing from the script"
        );
    }
    // The first step waits on the boot line, not on a wall-clock offset.
    let first = text
        .lines()
        .find(|l| l.starts_with("after "))
        .expect("a first step");
    assert!(
        first.contains("[RECOVERY] boot complete (first frame served)"),
        "{first}"
    );
    // And the script lives where the README says.
    assert!(workspace_root().is_some());
}
