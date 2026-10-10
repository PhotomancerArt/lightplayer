//! RESEARCH (branch `research/ram-e11`, never for main): the choker under
//! the fragmenting sequence, on today's gate and on the lender.
//!
//! Experiment E11 of `lp2025/2026-10-09-1203-ram-research`. One sequence,
//! run against two images (`LP_E11_ELF`): the baseline (today's read gate,
//! with `e11_link_standin`) and the lender (`e11_lender,e11_link_standin`).
//!
//! 1. Load the choker; Studio's staged first sync; four lens rounds (each
//!    the four focuses and a card read).
//! 2. `LP_E11_EDITS` growing shader edits (default 10), each followed by a
//!    render-lens read and a card read. The link stand-in (a ~18 KB,
//!    link-shaped allocation from a task of its own) goes off after the
//!    30th served read, i.e. during the edits: a link opening mid-project.
//! 3. The choker's 27,091 B SVG read whole (the whole-file taker), and two
//!    more lens rounds.
//! 4. A switch: `projects/test/basic`, then the choker again; its first
//!    sync, two lens rounds, the SVG again.
//!
//! The driver prints `E11READ` per project read, `E11FILE` per whole-file
//! read, `E11PHASE` at each phase and `E11DONE` with the boot count; the
//! board's console (`LP_E11_CONSOLE`) carries the `[e11]` loan lines and
//! the heartbeat counters. The choker's PowerButton reads D0, so a pin
//! script holds D0 high from power-on. Figures are `lp-emu:esp32c6:t1`.

#[path = "support/editor_reads.rs"]
mod editor_reads;

use std::path::{Path, PathBuf};

use editor_reads::{EditorReads, LENS_GAP_US, block_on, deploy};
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost};
use lpa_client::LpClient;

const NONCE: u32 = 0xE11E_C6E5;
const BOOT_BANNER: &str = "[INIT] Initializing board...";

#[test]
#[ignore = "research: needs LP_E11_ELF (a fw-esp32c6 ELF with e11_link_standin)"]
fn e11_the_choker_under_the_fragmenting_sequence() {
    let Some(elf) = std::env::var_os("LP_E11_ELF").map(PathBuf::from) else {
        eprintln!("emu_lender: skipped — LP_E11_ELF not set");
        return;
    };
    let edits = env_u32("LP_E11_EDITS", 10);
    let choker_dir = repo_root().join("catalog/projects/playful-choker");
    let basic_dir = repo_root().join("projects/test/basic");

    let mut host = hosted(&elf);
    host.wait_for_line("\"hello\":{", 3_000_000)
        .expect("the boot")
        .expect("a hello");
    let mut files: Vec<String> = Vec::new();
    let mut reads = Vec::new();

    println!("E11PHASE load at {:.3} s", host.board_seconds());
    let choker = load(&mut host, &choker_dir);
    let shader = format!("{}/shader.glsl", choker.path.as_str());
    let svg = format!("{}/playful-mapping.svg", choker.path.as_str());
    {
        let mut editor = EditorReads::new(&mut host, choker.handle);
        editor.idle(2_000_000);
        println!("E11PHASE sync");
        editor.initial_sync();
        rounds(&mut editor, 4);
        println!("E11PHASE edits");
        let original = editor.fs_read(&shader);
        for edit in 1..=edits {
            editor.fs_write(&shader, edited_shader(&original, edit));
            editor.idle(1_000_000);
            editor.lens("render");
            editor.card_feed();
        }
        println!("E11PHASE whole-file");
        files.push(whole_file(&mut editor, &svg));
        rounds(&mut editor, 2);
        editor.fs_write(&shader, original);
        editor.idle(1_000_000);
        reads.append(&mut editor.reads);
    }

    println!("E11PHASE switch at {:.3} s", host.board_seconds());
    let basic = load(&mut host, &basic_dir);
    {
        let mut editor = EditorReads::new(&mut host, basic.handle);
        editor.idle(2_000_000);
        editor.card_feed();
        reads.append(&mut editor.reads);
    }
    let choker = load(&mut host, &choker_dir);
    {
        let mut editor = EditorReads::new(&mut host, choker.handle);
        editor.idle(2_000_000);
        println!("E11PHASE resync");
        editor.initial_sync();
        rounds(&mut editor, 2);
        files.push(whole_file(&mut editor, &svg));
        editor.card_feed();
        reads.append(&mut editor.reads);
    }
    let until = host.board.machine.micros() + 11_000_000;
    host.run_until(until, None).expect("the tail");

    for (index, read) in reads.iter().enumerate() {
        println!(
            "E11READ {index} {} at {:.3} s events {} ended {} error {}",
            read.label,
            read.at_s,
            read.events,
            read.ended,
            read.error.as_deref().unwrap_or("-")
        );
    }
    for file in &files {
        println!("E11FILE {file}");
    }
    let boots = host
        .console()
        .iter()
        .filter(|line| line.contains(BOOT_BANNER))
        .count();
    let refused = reads.iter().filter(|read| read.error.is_some()).count();
    println!(
        "E11DONE reads {} refused {refused} files {} boots {boots}",
        reads.len(),
        files.len()
    );
}

fn rounds(editor: &mut EditorReads<'_>, rounds: u32) {
    for _ in 0..rounds {
        for focus in ["none", "render", "control", "timebase"] {
            editor.lens(focus);
            editor.idle(LENS_GAP_US);
        }
        editor.card_feed();
        editor.idle(LENS_GAP_US);
    }
}

/// Read `path` whole; the line says what came back.
fn whole_file(editor: &mut EditorReads<'_>, path: &str) -> String {
    let at = editor.host.board_seconds();
    let lp_path = lpfs::LpPathBuf::from(path);
    let mut client = editor.client();
    match block_on(client.fs_read(lp_path.as_path())) {
        Ok(read) => format!("{path} at {at:.3} s: {} B", read.value.len()),
        Err(error) => format!("{path} at {at:.3} s: REFUSED {error}"),
    }
}

/// Deploy and load `dir`; the loaded project.
fn load(host: &mut EmuLinkHost<C6Board>, dir: &Path) -> lpc_wire::LoadedProject {
    deploy(host, dir);
    let mut client = LpClient::new(&mut *host).with_request_ids_from(1_000);
    let loaded = block_on(client.project_list_loaded()).expect("the loaded projects");
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    loaded
        .value
        .iter()
        .find(|project| project.path.as_str().contains(name))
        .or(loaded.value.first())
        .cloned()
        .expect("the deploy loaded it")
}

fn edited_shader(original: &[u8], edits: u32) -> Vec<u8> {
    let source = std::str::from_utf8(original).expect("UTF-8");
    let anchor = "    vec3 color = texture(palette";
    let at = source.find(anchor).expect("the choker's shader");
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
    match std::env::var_os("LP_E11_CONSOLE") {
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
