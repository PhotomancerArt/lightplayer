//! Emulator seams M0 part C: the USB census (spike; plan
//! `lp2025/2026-10-05-1026-emulator-seams`). What share of the emulator's
//! host time does the USB link layer cost in end-user browser simulation?
//!
//! The shipped image, booted ROM-up from a merged chip (the way a Studio tab
//! boots a board), `lp-emu:esp32c6:t2`, with Studio's side played by the
//! in-process host (`EmuLinkHost`, a fixed nonce, so two runs are the same
//! run): lp-link and `LpClient` on the host, `EditorReads` for Studio's
//! request shapes and cadence.
//!
//! Only the time spent **inside the machine** is counted (`C6Board`'s thread
//! CPU accounting): in a browser the host half runs in Studio's page, not in
//! the emulator's worker.
//!
//! Two methods per scenario:
//! 1. **Counterfactual**: the same emulated window with no host at all.
//! 2. **Attribution**: guest instructions by symbol (`blockprof`), USB
//!    driver/ISR/IN-endpoint gate against lp-link and the server.
//!
//! Inputs (all required; the test skips loudly without them):
//! `LP_M0_CHIP_EMPTY` (a merged image, empty lpfs), `LP_M0_CHIP_BASIC` (the
//! same with `projects/test/basic` as the startup project), `LP_M0_ELF` (the
//! image's ELF, symbols only). `#[ignore]`: a measurement, never a gate.

#[path = "support/editor_reads.rs"]
mod editor_reads;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use editor_reads::{EditorReads, LENS_GAP_US, block_on, deploy};
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost, thread_cpu};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{
    AppSource, BootMode, Esp32C6Builder, Esp32C6Machine, StopCondition, TimeGrade, UsbHost,
};
use lp_emu_esp32c6::memmap;
use lpa_client::LpClient;

const NONCE: u32 = 0x5EA0_C0DE;
/// Where every window starts: past the ROM-up boot and the project's compile.
const MARK_US: u64 = 3_000_000;
/// The measured window.
const WINDOW_US: u64 = 5_000_000;

#[test]
#[ignore = "a measurement (part C of the emulator-seams spike); needs LP_M0_* images"]
fn usb_census() {
    let (Some(empty), Some(basic), Some(elf)) = (
        std::env::var_os("LP_M0_CHIP_EMPTY").map(PathBuf::from),
        std::env::var_os("LP_M0_CHIP_BASIC").map(PathBuf::from),
        std::env::var_os("LP_M0_ELF").map(PathBuf::from),
    ) else {
        eprintln!("usb_census: skipped — set LP_M0_CHIP_EMPTY, LP_M0_CHIP_BASIC, LP_M0_ELF");
        return;
    };
    let only = std::env::var("LP_M0_SCENARIOS").unwrap_or_else(|_| "c1,c2,c3".into());
    let profile = std::env::var_os("LP_M0_BLOCKPROF").is_some();
    println!(
        "usb census: lp-emu:esp32c6:t2, ROM-up, window {} s after {} s; blockprof {profile}",
        WINDOW_US as f64 / 1e6,
        MARK_US as f64 / 1e6
    );

    if only.contains("c1") {
        let absent = absent(&empty, &elf, profile);
        let quiet = hosted(&empty, &elf, profile, Drive::Quiet);
        let polled = hosted(&empty, &elf, profile, Drive::ListEvery150ms);
        report(
            "C1 idle board (no project), host attached",
            &absent,
            &[&quiet, &polled],
        );
    }
    if only.contains("c2") {
        let absent = absent(&basic, &elf, profile);
        let quiet = hosted(&basic, &elf, profile, Drive::Quiet);
        let lens = hosted(&basic, &elf, profile, Drive::Lens);
        report(
            "C2 render-basic rendering, host attached",
            &absent,
            &[&quiet, &lens],
        );
    }
    if only.contains("c3") {
        let quiet = hosted(&empty, &elf, profile, Drive::Quiet);
        let upload = hosted(&empty, &elf, profile, Drive::BulkUpload);
        report("C3 bulk upload", &quiet, &[&upload]);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drive {
    /// The link up (hello, keepalives), nothing asked.
    Quiet,
    /// The project list every 150 ms: a device card with nothing open.
    ListEvery150ms,
    /// Studio's editor lens at its cadence, plus the card feed.
    Lens,
    /// A large project deployed, then a large file written.
    BulkUpload,
}

/// One measured window.
struct Window {
    name: String,
    /// Thread CPU inside the machine over the window.
    cpu: Duration,
    emulated_us: u64,
    instructions: u64,
    frames: usize,
    idle_skips: u64,
    /// Guest instructions retired in the window, by demangled symbol.
    symbols: BTreeMap<String, u64>,
    note: String,
}

fn builder(chip: &PathBuf, elf: &PathBuf, profile: bool, host: UsbHost) -> Esp32C6Builder {
    let len = std::fs::metadata(chip).expect("the chip image").len() as u32;
    Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash(FlashBacking::Copy(chip.clone()))
        .flash_len(len)
        // A symbol table only on a ROM-up boot.
        .app(AppSource::Path(elf.clone()))
        .time_grade(TimeGrade::T2)
        .blockprof(profile)
        .usb_host(host)
}

fn absent(chip: &PathBuf, elf: &PathBuf, profile: bool) -> Window {
    let mut m = builder(chip, elf, profile, UsbHost::Absent)
        .build()
        .expect("a machine");
    run_to(&mut m, MARK_US);
    let before = snapshot(&m);
    let t0 = thread_cpu();
    run_to(&mut m, MARK_US + WINDOW_US);
    let cpu = thread_cpu() - t0;
    window(&m, "no host (counterfactual)", cpu, before, String::new())
}

fn hosted(chip: &PathBuf, elf: &PathBuf, profile: bool, drive: Drive) -> Window {
    let machine = builder(chip, elf, profile, UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .build()
        .expect("a machine");
    let board = C6Board::new(machine)
        .expect("a hosted board")
        .time_cpu(true);
    let mut host = EmuLinkHost::new(board, NONCE, true);
    host.wait_for_line("\"hello\":{", MARK_US)
        .expect("the boot")
        .expect("a hello before the mark");
    let mut handle = None;
    if drive == Drive::Lens {
        let loaded = {
            let mut client = LpClient::new(&mut host).with_request_ids_from(1_000);
            block_on(client.project_list_loaded()).expect("the loaded projects")
        };
        handle = Some(
            loaded
                .value
                .first()
                .expect("the startup project loaded")
                .handle,
        );
    }
    // The staged first sync belongs to the boot, not the window.
    if let Some(handle) = handle {
        let mut editor = EditorReads::new(&mut host, handle);
        editor.initial_sync();
    }
    host.run_until(MARK_US, None).expect("to the mark");
    let before = snapshot(&host.board.machine);
    let cpu0 = host.board.cpu_in_machine;
    let end = MARK_US + WINDOW_US;
    let mut note = String::new();
    match drive {
        Drive::Quiet => {
            host.run_until(end, None).expect("the window");
        }
        Drive::ListEvery150ms => {
            let mut n = 0u64;
            while host.board.machine.micros() < end {
                let mut client = LpClient::new(&mut host).with_request_ids_from(10_000 + n * 4);
                block_on(client.project_list_loaded()).expect("a list");
                n += 1;
                let until = host.board.machine.micros() + 150_000;
                host.run_until(until.min(end), None).expect("the gap");
            }
            note = format!("{n} project-list reads");
        }
        Drive::Lens => {
            let handle = handle.unwrap();
            let mut editor = EditorReads::new(&mut host, handle);
            let mut n = 0u64;
            while editor.host.board.machine.micros() < end {
                editor.lens("render");
                n += 1;
                if n % 2 == 0 {
                    editor.card_feed();
                }
                editor.idle(LENS_GAP_US);
            }
            let failed = editor.reads.iter().filter(|r| r.error.is_some()).count();
            note = format!("{} reads ({n} lens), {failed} failed", editor.reads.len());
        }
        Drive::BulkUpload => {
            let dir = bulk_project();
            let t = host.board.machine.micros();
            deploy(&mut host, &dir);
            let deployed = host.board.machine.micros();
            note = format!(
                "deploy of {} took {:.3} s emulated",
                dir.display(),
                (deployed - t) as f64 / 1e6
            );
            let left = end.saturating_sub(host.board.machine.micros());
            if left > 0 {
                host.run_until(end, None).expect("the window's tail");
            }
        }
    }
    let cpu = host.board.cpu_in_machine - cpu0;
    let name = format!("host attached, {drive:?}");
    window(&host.board.machine, &name, cpu, before, note)
}

/// A project of about half the C6's 704 KB filesystem: `projects/test/basic`
/// plus incompressible filler. (The brief asked for ≥ 1 MB plus a 1 MB file;
/// the C6's lpfs partition is 0xB0000 bytes, so it cannot hold that.)
fn bulk_project() -> PathBuf {
    let root = std::env::temp_dir().join(format!("lp-m0-bulk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let basic = repo_root().join("projects/test/basic");
    for entry in std::fs::read_dir(&basic).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), root.join(entry.file_name())).unwrap();
    }
    let kb: usize = std::env::var("LP_M0_BULK_KB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(320);
    let mut filler = vec![0u8; kb * 1024];
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    for b in filler.iter_mut() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = x as u8;
    }
    std::fs::write(root.join("filler.bin"), filler).unwrap();
    root
}

fn run_to(m: &mut Esp32C6Machine, us: u64) {
    let stop = StopCondition {
        stop_cycle: Some(us * memmap::CYCLES_PER_US),
        ..Default::default()
    };
    m.run_until(&stop);
}

struct Before {
    micros: u64,
    instructions: u64,
    frames: usize,
    idle_skips: u64,
    symbols: BTreeMap<String, u64>,
}

fn snapshot(m: &Esp32C6Machine) -> Before {
    Before {
        micros: m.micros(),
        instructions: m.instructions(),
        frames: m.frames(18).len(),
        idle_skips: m.idle_skips(),
        symbols: m
            .blockprof_symbols()
            .map(|v| v.into_iter().collect())
            .unwrap_or_default(),
    }
}

fn window(m: &Esp32C6Machine, name: &str, cpu: Duration, before: Before, note: String) -> Window {
    let mut symbols = BTreeMap::new();
    if let Some(now) = m.blockprof_symbols() {
        for (sym, n) in now {
            let d = n - before.symbols.get(&sym).copied().unwrap_or(0);
            if d > 0 {
                symbols.insert(sym, d);
            }
        }
    }
    Window {
        name: name.to_string(),
        cpu,
        emulated_us: m.micros() - before.micros,
        instructions: m.instructions() - before.instructions,
        frames: m.frames(18).len() - before.frames,
        idle_skips: m.idle_skips() - before.idle_skips,
        symbols,
        note,
    }
}

/// Which layer a guest symbol belongs to, at spike precision.
fn class(sym: &str) -> &'static str {
    let s = sym;
    if s.contains("usb_serial_jtag")
        || s.contains("UsbSerialJtag")
        || s.contains("InEndpoint")
        || s.contains("in_endpoint")
        || s.contains("USB_DEVICE")
        || s.contains("usb_connection")
    {
        "usb (driver/ISR/IN gate)"
    } else if s.contains("usb_link") {
        "usb link task (glue)"
    } else if s.contains("lp_link") {
        "lp-link"
    } else if s.contains("lpa_server")
        || s.contains("lpc_wire")
        || s.contains("lp_json_pack")
        || s.contains("serde")
        || s.contains("lpc_view")
    {
        "server + wire"
    } else {
        "other"
    }
}

fn report(title: &str, base: &Window, runs: &[&Window]) {
    println!("\n== {title}");
    for w in std::iter::once(base).chain(runs.iter().copied()) {
        println!(
            "  {:<40} cpu {:>7.3} s over {:.2} s emulated, {:>11} instr, {:>4} frames, {:>7} idle \
             skips{}",
            w.name,
            w.cpu.as_secs_f64(),
            w.emulated_us as f64 / 1e6,
            w.instructions,
            w.frames,
            w.idle_skips,
            if w.note.is_empty() {
                String::new()
            } else {
                format!("  ({})", w.note)
            }
        );
    }
    for w in runs {
        let delta = w.cpu.as_secs_f64() - base.cpu.as_secs_f64();
        println!(
            "  counterfactual: {} vs {}: +{:.3} s = {:.1} % of the attached run's emulator time",
            w.name,
            base.name,
            delta,
            100.0 * delta / w.cpu.as_secs_f64()
        );
        if w.symbols.is_empty() {
            continue;
        }
        let total: u64 = w.symbols.values().sum();
        let mut by_class: BTreeMap<&str, u64> = BTreeMap::new();
        for (sym, n) in &w.symbols {
            *by_class.entry(class(sym)).or_default() += n;
        }
        println!(
            "  attribution ({}): guest instructions by layer: {}",
            w.name,
            by_class
                .iter()
                .map(|(c, n)| format!("{c} {:.2} %", 100.0 * *n as f64 / total.max(1) as f64))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let mut extra: Vec<(&String, i64)> = w
            .symbols
            .iter()
            .map(|(s, n)| {
                (
                    s,
                    *n as i64 - base.symbols.get(s).copied().unwrap_or(0) as i64,
                )
            })
            .filter(|(_, d)| *d > 0)
            .collect();
        extra.sort_by(|a, b| b.1.cmp(&a.1));
        println!("  top symbols the host added (vs {}):", base.name);
        for (s, d) in extra.iter().take(25) {
            println!("    {:>10} {:<26} {}", d, class(s), s);
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}
