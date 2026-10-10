//! RESEARCH (branch `research/ram-e07`, never for main): the real peak of
//! every job a memory gate guards, on the emulated C6.
//!
//! Experiment E7 of `lp2025/2026-10-09-1203-ram-research`. The run is
//! `emu_frag_reads`'s conversation, shortened and labelled per read, against
//! an `alloc_watch_diag` image: each `project-read`, `shader-compile` and
//! `project-load` window logs `[aw] <kind>#<seq> … pk=<peak above start>`
//! with its five largest asks. The driver prints one `E7READ <n> <label>`
//! line per read so the analysis (`e07-peaks.py` in the evidence folder)
//! pairs the board's n-th read window with what was asked.
//!
//! Environment:
//! - `LP_E7_ELF` — the `alloc_watch_diag` ELF (required; skips without it);
//! - `LP_E7_CONSOLE` — where to write the board's whole console;
//! - `LP_E7_PROJECT` — the project directory (default
//!   `catalog/projects/playful-choker`);
//! - `LP_E7_ROUNDS` / `LP_E7_EDITS` — lens rounds and growing shader edits
//!   (default 4 and 4).
//!
//! The choker's PowerButton reads D0; an emulated pad nobody drives reads
//! low, so the pin script holds D0 high from power-on.

#[path = "support/editor_reads.rs"]
mod editor_reads;

use std::path::{Path, PathBuf};

use editor_reads::{EditorReads, LENS_GAP_US, block_on, deploy};
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost};
use lpa_client::LpClient;

const NONCE: u32 = 0xE7E7_C6E5;

#[test]
#[ignore = "research: needs LP_E7_ELF (an alloc_watch_diag fw-esp32c6 ELF)"]
fn e07_the_real_peak_of_each_guarded_job() {
    let Some(elf) = std::env::var_os("LP_E7_ELF").map(PathBuf::from) else {
        eprintln!("emu_gate_peaks: skipped — LP_E7_ELF not set");
        return;
    };
    let project_dir = std::env::var_os("LP_E7_PROJECT")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root().join("catalog/projects/playful-choker"));
    let rounds = env_u32("LP_E7_ROUNDS", 4);
    let edits = env_u32("LP_E7_EDITS", 4);

    let mut host = hosted(&elf);
    host.wait_for_line("\"hello\":{", 3_000_000)
        .expect("the boot")
        .expect("a hello");
    println!("E7PHASE deploy at {:.3} s", host.board_seconds());
    deploy(&mut host, &project_dir);
    let loaded = {
        let mut client = LpClient::new(&mut host).with_request_ids_from(1_000);
        block_on(client.project_list_loaded()).expect("the loaded projects")
    };
    let project = loaded.value.first().cloned().expect("the deploy loaded it");
    let shader = format!("{}/shader.glsl", project.path.as_str());

    let mut editor = EditorReads::new(&mut host, project.handle);
    // Two frames' worth of settling, so the boot compile is behind us.
    editor.idle(2_000_000);
    println!("E7PHASE sync");
    editor.initial_sync();
    println!("E7PHASE rounds");
    for _ in 0..rounds {
        for focus in ["none", "render", "control", "timebase"] {
            editor.lens(focus);
            editor.idle(LENS_GAP_US);
        }
        editor.card_feed();
        editor.idle(LENS_GAP_US);
    }
    println!("E7PHASE edits");
    let original = editor.fs_read(&shader);
    for edit in 1..=edits {
        editor.fs_write(&shader, edited_shader(&original, edit));
        // The compile window opens on the next tick; let it run.
        editor.idle(1_000_000);
        editor.lens("render");
        editor.card_feed();
    }
    editor.fs_write(&shader, original);
    editor.idle(1_000_000);
    editor.lens("none");
    editor.card_feed();
    let reads = std::mem::take(&mut editor.reads);
    drop(editor);
    // Past the next heartbeat, so the ring drains.
    let until = host.board.machine.micros() + 6_000_000;
    host.run_until(until, None).expect("the tail");
    for read in &reads {
        println!(
            "E7READ {} {} at {:.3} s events {} ended {} error {}",
            read.index,
            read.label,
            read.at_s,
            read.events,
            read.ended,
            read.error.as_deref().unwrap_or("-")
        );
    }
    let boots = host
        .console()
        .iter()
        .filter(|line| line.contains("[INIT] Initializing board..."))
        .count();
    println!("E7DONE reads {} boots {boots}", reads.len());
}

fn edited_shader(original: &[u8], edits: u32) -> Vec<u8> {
    let source = std::str::from_utf8(original).expect("UTF-8");
    let anchor = "    vec3 color = texture(palette";
    let Some(at) = source.find(anchor) else {
        // Not the choker: append a comment-free growing line at the end of
        // the file instead is not valid GLSL, so edit nothing.
        return original.to_vec();
    };
    let mut edited = String::from(&source[..at]);
    for edit in 1..=edits {
        edited.push_str(&format!(
            "    lum *= 1.0 + 0.01 * sin(p.x * {edit}.0 + time);\n"
        ));
    }
    edited.push_str(&source[at..]);
    edited.into_bytes()
}

fn hosted(elf: &Path) -> EmuLinkHost<C6Board> {
    let pins =
        lp_emu_esp32c6::pinscript::parse_pin_script("0 pin 0 1\n").expect("the D0 pin script");
    let machine = Esp32C6Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .flash(FlashBacking::Blank)
        .strict(false)
        .time_grade(TimeGrade::T1)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .pin_script(pins)
        .build()
        .expect("the image builds a machine");
    let host = EmuLinkHost::new(C6Board::new(machine).expect("a hosted board"), NONCE, true);
    match std::env::var_os("LP_E7_CONSOLE") {
        Some(path) => host.with_console_sink(Box::new(
            std::fs::File::create(path).expect("the console file"),
        )),
        None => host,
    }
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli sits under the repo root")
        .to_path_buf()
}
