//! The C6's main stack against what a project asks of it: the high-water
//! across a project load, its first frames and its shader compiles, graded
//! against the stack the image actually has, with a stated margin.
//!
//! The main stack is the residual of main RAM after the statics and the
//! heap (`lp-fw/fw-esp32c6/src/board/esp32c6/init.rs`), so every new static
//! and every byte given to `HEAP_MAIN` comes out of it. The chip heap
//! ratchet grades only the idle first heartbeat's high-water
//! (`scripts/heap-budget-check.sh`, "The stack's size is derived"), which a
//! loaded project exceeds three times over, so until this test nothing failed
//! when the stack fell below what the flagship needs
//! (`docs/defects/2026-10-09-the-c6-main-stack-margin-is-unguarded.md`).
//!
//! The run, per project: the shipped split image (loader direct-loaded over
//! the flashed chip, as the heap ratchet boots it) with this process as the
//! host on its USB link; the project deployed over the link (which loads
//! it); [`SETTLE_US`] of frames, past two heartbeats; then, when the project
//! gets edits, Studio's staged first sync and that many shader edits, each a
//! recompile and each a function longer than the last, with an editor-lens
//! read between them, and past one more heartbeat. The figure is the firmware's own `[stack]
//! heartbeat: high-water N B of M B` line (`fw-esp32c6/src/stack_probe.rs`:
//! a painted watermark, so it holds the deepest point any frame or interrupt
//! reached on the main task since boot).
//!
//! **The gate** ([`GATE_PROJECTS`], in CI's `Emulator C6 lp-cli (x64)` job
//! through `just test-emu-c6-cli-boards`): every project's peak plus
//! [`MARGIN_B`] must fit in the stack, and the board must not reset. A
//! failure says it in words: "meteor's load and 10 edits peak at N of M B;
//! margin K B < 12288 B". Emulated stack bytes transfer to silicon (the RAM
//! research found the memory classes within ~1 % wherever both were
//! measured, and meteor's high-water at ~35.7 KB on both), but they are
//! `lp-emu:esp32c6:t1`, never a hardware result, and that is the label every
//! figure here carries.
//!
//! **The census** (`LP_STACK_PEAK_CENSUS=1`): the same run for every project
//! under `catalog/` (or `LP_STACK_PEAK_PROJECTS=<a,b>`), edits only with
//! `LP_STACK_PEAK_EDITS=<n>`, printed as a table. It is how the gate's
//! projects were chosen, and it asserts nothing.
//!
//! `#[ignore]`d: it needs a split build (`LP_EMU_BUILD_FW=1`, or
//! `LP_EMU_C6_SPLIT_ESP32C6_SERVER_RADIO=<dir>`).

#[path = "support/editor_reads.rs"]
mod editor_reads;

use std::path::{Path, PathBuf};

use editor_reads::{EditorReads, block_on, deploy};
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost};
use lp_emu_esp32c6::test_support::{FwImage, SplitImage, split_image};
use lpa_client::LpClient;

/// What the stack must keep free above a project's measured peak: the
/// margin the C6 stack had over meteor's peak when the RAM split was last
/// decided (2026-09-24 → 2026-10-09: ~12 KB of 49 KB), and the margin the
/// RAM research's Q3 ruling (2026-10-16) keeps when the freed stack room
/// becomes heap.
const MARGIN_B: u64 = 12 * 1024;

/// The projects the gate loads, and the shader edits each gets: the two
/// deepest the census found (`LP_STACK_PEAK_CENSUS=1`, 2026-10-16, main at
/// `f3a77691e`, `lp-emu:esp32c6:t1`). Meteor peaks at 35,752 B (its compute
/// node's tick), the same after ten edits; the choker tryout (25 playlist
/// entries) at 33,852 B on its load. Every other catalog project that loads
/// on the emulator peaks at 30,652–32,252 B. The tryout gets no edits: on
/// main its editor sync after the load runs the heap out (an `[OOM]
/// FRAGMENTED` reset), which is a heap finding, not a stack one.
const GATE_PROJECTS: &[(&str, u32)] = &[
    ("catalog/patterns/meteor", 10),
    ("catalog/projects/playful-choker-tryout", 0),
];

/// Emulated time after the deploy before the edits: the deferred first
/// compile, frames, and two 5 s heartbeats.
const SETTLE_US: u64 = 12_000_000;

/// Emulated time between two edits: the recompile lands and a frame runs.
const EDIT_GAP_US: u64 = 1_000_000;

/// Past the next 5 s heartbeat, so its `[stack]` line covers what came
/// before it.
const PAST_A_HEARTBEAT_US: u64 = 6_000_000;

/// One fixed host nonce, so a run is a function of the image.
const NONCE: u32 = 0x57AC_C6E1;

/// The boot banner: a second one is a reset.
const BOOT_BANNER: &str = "[INIT] Initializing board...";

const STACK_LINE: &str = "[stack] heartbeat: high-water ";

const CONFIGURATION: &str = "lp-emu:esp32c6:t1";

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6-cli-boards` runs it"]
fn the_main_stack_holds_the_deepest_projects_with_a_12_kib_margin() {
    let Some(split) = image() else { return };
    let mut failures = Vec::new();
    let mut rows = Vec::new();
    for (project, edits) in GATE_PROJECTS {
        let run = measure(&split, project, *edits);
        rows.push(run.row());
        failures.extend(run.failures());
    }
    println!("{}", table(&rows));
    assert!(
        failures.is_empty(),
        "the C6 main stack gate ({CONFIGURATION}):\n  {}\n\n{}\nThe main stack is what main \
         RAM leaves after the statics and HEAP_MAIN (lp-fw/fw-esp32c6/src/board/esp32c6/init.rs). \
         Give it back from HEAP_MAIN, or make the path shallower; never lower the margin to pass \
         (docs/defects/2026-10-09-the-c6-main-stack-margin-is-unguarded.md).",
        failures.join("\n  "),
        table(&rows)
    );
}

#[test]
#[ignore = "a measurement: LP_STACK_PEAK_CENSUS=1, and a split fw-esp32c6 build"]
fn stack_peak_census() {
    if std::env::var_os("LP_STACK_PEAK_CENSUS").is_none() {
        eprintln!("emu_stack_peak: census skipped — set LP_STACK_PEAK_CENSUS=1");
        return;
    }
    let Some(split) = image() else { return };
    let edits = std::env::var("LP_STACK_PEAK_EDITS")
        .ok()
        .map(|n| n.parse::<u32>().expect("LP_STACK_PEAK_EDITS is a count"))
        .unwrap_or(0);
    let projects = match std::env::var("LP_STACK_PEAK_PROJECTS") {
        Ok(list) => list.split(',').map(str::to_string).collect(),
        Err(_) => catalog_projects(),
    };
    let mut rows = Vec::new();
    for project in &projects {
        let run = measure(&split, project, edits);
        let row = run.row();
        println!("{row}");
        rows.push(row);
    }
    println!("{}", table(&rows));
}

/// One project's run.
struct Run {
    project: String,
    /// `of M B`: the stack the image has.
    total: Option<u64>,
    /// The high-water at the first heartbeat (boot, no project).
    idle: Option<u64>,
    /// After the deploy, its compiles and [`SETTLE_US`] of frames.
    loaded: Option<u64>,
    /// After the edits (equal to `loaded` with none).
    edited: Option<u64>,
    edits: u32,
    boots: usize,
    link_errors: u32,
    /// What went wrong outside the stack (a failed deploy, a reset).
    trouble: Option<String>,
}

impl Run {
    fn peak(&self) -> Option<u64> {
        self.edited.or(self.loaded)
    }

    fn row(&self) -> String {
        let num = |v: Option<u64>| v.map_or_else(|| "?".to_string(), |v| v.to_string());
        let spare = match (self.total, self.peak()) {
            (Some(total), Some(peak)) => (total as i64 - peak as i64).to_string(),
            _ => "?".to_string(),
        };
        format!(
            "| {} | {} | {} | {} | {} ({} edits) | {} | {} | {} |",
            self.project,
            num(self.total),
            num(self.idle),
            num(self.loaded),
            num(self.edited),
            self.edits,
            spare,
            self.boots,
            self.trouble.as_deref().unwrap_or("")
        )
    }

    /// The gate's verdict, in words.
    fn failures(&self) -> Vec<String> {
        let mut out = Vec::new();
        let name = self
            .project
            .rsplit('/')
            .next()
            .unwrap_or(&self.project)
            .to_string();
        if let Some(trouble) = &self.trouble {
            out.push(format!("{name}: {trouble}"));
        }
        if self.boots != 1 {
            out.push(format!(
                "{name}: the board booted {} times (a reset)",
                self.boots
            ));
        }
        match (self.total, self.peak()) {
            (Some(total), Some(peak)) => {
                let margin = total as i64 - peak as i64;
                if margin < MARGIN_B as i64 {
                    let what = match self.edits {
                        0 => "load".to_string(),
                        n => format!("load and {n} edits"),
                    };
                    out.push(format!(
                        "{name}'s {what} peak at {peak} of {total} B; margin {margin} B < \
                         {MARGIN_B} B ({} KiB)",
                        MARGIN_B / 1024
                    ));
                }
            }
            _ => out.push(format!(
                "{name}: no `[stack] heartbeat` line after the load (idle {:?}, loaded {:?}, \
                 edited {:?})",
                self.idle, self.loaded, self.edited
            )),
        }
        out
    }
}

/// Boot a fresh board, deploy `project`, run it, then edit its shader
/// `edits` times, reading the stack's high-water after each phase.
fn measure(split: &SplitImage, project: &str, edits: u32) -> Run {
    let dir = repo_root().join(project);
    let mut host = hosted(split);
    let mut run = Run {
        project: project.to_string(),
        total: None,
        idle: None,
        loaded: None,
        edited: None,
        edits,
        boots: 0,
        link_errors: 0,
        trouble: None,
    };
    let first = host
        .wait_for_line(STACK_LINE, 12_000_000)
        .expect("the boot");
    if first.is_none() {
        run.trouble = Some("no first heartbeat".to_string());
        return finish(run, &host);
    }
    (run.idle, run.total) = high_water(host.console());

    let deployed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        deploy(&mut host, &dir);
    }));
    if deployed.is_err() {
        run.trouble = Some("the deploy failed".to_string());
        return finish(run, &host);
    }
    let until = host.board.machine.micros() + SETTLE_US;
    if let Err(error) = host.run_until(until, None) {
        run.loaded = high_water(host.console()).0;
        run.trouble = Some(format!("the run stopped after the deploy: {error:#}"));
        return finish(run, &host);
    }
    run.loaded = high_water(host.console()).0;

    if edits > 0 {
        let loaded = {
            let mut client = LpClient::new(&mut host).with_request_ids_from(1_000);
            block_on(client.project_list_loaded()).expect("the loaded projects")
        };
        let Some(on_board) = loaded.value.first().cloned() else {
            run.trouble = Some("the deploy loaded nothing".to_string());
            return finish(run, &host);
        };
        let relative = edited_shader_path(&dir);
        let shader = format!("{}/{relative}", on_board.path.as_str());
        let session = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut editor = EditorReads::new(&mut host, on_board.handle);
            editor.initial_sync();
            let original = editor.fs_read(&shader);
            for edit in 1..=edits {
                editor.fs_write(&shader, edited_shader(&original, edit));
                editor.idle(EDIT_GAP_US);
                editor.lens("render");
            }
        }));
        if session.is_err() {
            run.edited = high_water(host.console()).0;
            run.trouble = Some(format!("the edit session failed (editing {relative})"));
            return finish(run, &host);
        }
        let until = host.board.machine.micros() + PAST_A_HEARTBEAT_US;
        if let Err(error) = host.run_until(until, None) {
            run.trouble = Some(format!("the run stopped after the edits: {error:#}"));
        }
        run.edited = high_water(host.console()).0;
    }
    finish(run, &host)
}

fn finish(mut run: Run, host: &EmuLinkHost<C6Board>) -> Run {
    run.boots = host
        .console()
        .iter()
        .filter(|line| line.contains(BOOT_BANNER))
        .count();
    run.link_errors = host.link_errors;
    if let Ok(path) = std::env::var("LP_STACK_PEAK_CONSOLE_DIR") {
        let name = run.project.replace('/', "_");
        let _ = std::fs::write(
            Path::new(&path).join(format!("{name}.console.txt")),
            host.console().join("\n"),
        );
    }
    run
}

/// The deepest `[stack] heartbeat` line so far: `(used, total)`.
fn high_water(console: &[String]) -> (Option<u64>, Option<u64>) {
    let mut best: (Option<u64>, Option<u64>) = (None, None);
    for line in console {
        let Some(at) = line.find(STACK_LINE) else {
            continue;
        };
        let mut words = line[at + STACK_LINE.len()..].split_whitespace();
        let used = words.next().and_then(|w| w.parse::<u64>().ok());
        let total = words.nth(2).and_then(|w| w.parse::<u64>().ok());
        if used > best.0 {
            best = (used, total);
        }
    }
    best
}

/// The shader file the edits rewrite: `LP_STACK_PEAK_SHADER`, or the
/// project's first `.glsl` in path order.
fn edited_shader_path(dir: &Path) -> String {
    if let Ok(path) = std::env::var("LP_STACK_PEAK_SHADER") {
        return path;
    }
    let mut found = Vec::new();
    collect_glsl(dir, dir, &mut found);
    found.sort();
    found
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("{} has no .glsl to edit", dir.display()))
}

fn collect_glsl(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("the project's directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            collect_glsl(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "glsl") {
            out.push(
                path.strip_prefix(root)
                    .expect("under the project")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

/// The shader after `edits` edits: one more function each time, so every
/// recompile has a little more to do than the last (a comment-only edit
/// compiles to the same code).
fn edited_shader(original: &[u8], edits: u32) -> Vec<u8> {
    let mut edited = String::from_utf8(original.to_vec()).expect("the shader is UTF-8");
    edited.push('\n');
    for edit in 1..=edits {
        edited.push_str(&format!(
            "float lp_stack_edit_{edit}(float x) {{ return x * {edit}.0 + sin(x * 0.5); }}\n"
        ));
    }
    edited.into_bytes()
}

/// Every project directory under `catalog/` (patterns and projects).
fn catalog_projects() -> Vec<String> {
    let root = repo_root();
    let mut out = Vec::new();
    for group in ["catalog/patterns", "catalog/projects"] {
        let mut names: Vec<_> = std::fs::read_dir(root.join(group))
            .expect("the catalog")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().join("project.json").is_file())
            .map(|e| format!("{group}/{}", e.file_name().to_string_lossy()))
            .collect();
        names.sort();
        out.extend(names);
    }
    out
}

fn table(rows: &[String]) -> String {
    format!(
        "main stack high-water, {CONFIGURATION} (B)\n| project | stack | idle | loaded | edited \
         | stack − peak | boots | trouble |\n|---|---:|---:|---:|---:|---:|---:|---|\n{}",
        rows.join("\n")
    )
}

/// The split image direct-loaded over its flashed chip — the boot the chip
/// heap ratchet uses (`scripts/heap-budget-check.sh`: `--elf loader.elf
/// --over merged.bin --mmu-page 32k`) — attached and draining from power-on,
/// with this process as the host on its link. The flashed chip holds no
/// device store, so Bluetooth is on, as on a fresh board.
fn hosted(split: &SplitImage) -> EmuLinkHost<C6Board> {
    let len = std::fs::metadata(split.merged()).unwrap().len() as u32;
    let machine = Esp32C6Builder::new()
        .app(AppSource::Path(split.loader_elf()))
        .flash(FlashBacking::Copy(split.merged()))
        .flash_len(len)
        .flash_holds_image(true)
        .mmu_page_len(0x8000)
        // Not strict: this is about the stack, not the bus, and a strict bus
        // is several times slower for the same figures.
        .strict(false)
        .time_grade(TimeGrade::T1)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .build()
        .expect("the split image builds a machine");
    EmuLinkHost::new(C6Board::new(machine).expect("a hosted board"), NONCE, true)
}

fn image() -> Option<SplitImage> {
    match split_image(&FwImage::SHIPPED) {
        Ok(split) => Some(split),
        Err(reason) => {
            eprintln!("emu_stack_peak: skipped — {reason}");
            None
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli sits under the repo root")
        .to_path_buf()
}
