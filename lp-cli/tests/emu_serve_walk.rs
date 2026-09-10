//! Plan two M1's **gate**: the upload walk over the WebSocket door lands the
//! same project with the same figures as M6 P5's `upload-walk-usb` over TCP.
//!
//! `lp-emu/esp/lp-emu-esp32c6/tests/upload_walk_usb.rs` replays a
//! **deterministic script** (`walks/examples-basic.script`) through the USB
//! link and pins the figures the walk produced. This runs the **same project**
//! through a **live** `lp-cli upload … serial:ws://…` against
//! `lp-cli emu serve`, and pins the **same figures**.
//!
//! It deliberately does **not** pin the interleaving, a cycle, or a duration.
//! The script's bytes land where the file says; a live client's land where
//! the host's clock puts them, and `lp-emu/esp/README.md` §Determinism is why.
//! Equal figures, not equal bytes.
//!
//! Two `#[ignore]`d tests, because between them they want a riscv32 build of
//! a pinned firmware commit and a `git archive` of a project the catalog has
//! since moved.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use support::{Serve, reference_elf, scratch, skip};

/// §5.3, verbatim — and `upload_walk_usb.rs`'s, verbatim. The link driver is
/// not in the heap ledger, and this is what says so over a third link.
const LOAD_AFTER: &str = "[mem] load_project after: 220532 B free / 105004 B used (215k / 102k)";

/// The compiler's own outputs, verbatim.
const COMPILE_OUTPUTS: &str = "lpir_inst_count=573, lpir_func_count=12, lpir_import_count=7, \
     final_inst_count=2048, final_code_size=8192 bytes, float=fixed)";

/// The commit the walk was captured against. The project catalog moved since,
/// so the project comes out of git rather than off disk —
/// `lp-emu/esp/lp-emu-esp32c6/walks/README.md` says the same.
const WALK_COMMIT: &str = "d6cfaa205";

/// **Gate 4** — the upload walk over the WS door, and **gate 5** — the second
/// boot finds the project.
///
/// One test, because gate 5 is only meaningful as the *second* half of gate
/// 4: it asks whether the bytes the first server wrote back survived it.
#[test]
#[ignore = "needs the shipped reference image and a git archive; run through `just test-emu-serve`"]
fn the_walk_over_the_ws_door_lands_the_same_project_with_the_same_figures() {
    let Some(elf) = reference_elf("emu_serve_walk") else {
        return;
    };
    let Some(project) = walk_project() else {
        return;
    };
    let state = scratch();
    let _ = std::fs::remove_dir_all(&state);

    // ---- gate 4 --------------------------------------------------------
    let serve = Serve::start_in(&elf, &["c6-a", "c6-b"], &[], state.clone());
    assert_eq!(
        serve.board("c6-a")["flash"],
        "blank",
        "a fresh state dir is a blank board"
    );

    let host = format!("serial:ws://127.0.0.1:{}/board/c6-a/bytes", serve.port());
    let upload = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
        .arg("upload")
        .arg(&project)
        .arg(&host)
        .output()
        .expect("running lp-cli upload");
    let told = format!(
        "--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&upload.stdout),
        String::from_utf8_lossy(&upload.stderr)
    );
    assert!(
        upload.status.success(),
        "`lp-cli upload {project:?} {host}` exited {:?}\n{told}",
        upload.status.code()
    );

    // The board's own console, not the client's report of it.
    let console = serve.wait_for_console("c6-a", "[shader-node] compilation succeeded");
    assert!(
        console.contains(LOAD_AFTER),
        "the heap ledger moved. Expected:\n  {LOAD_AFTER}\nBoard c6-a's console:\n{console}"
    );
    assert!(
        console.contains(COMPILE_OUTPUTS),
        "the compiler's outputs moved. Expected:\n  {COMPILE_OUTPUTS}\nBoard c6-a's console:\n{console}"
    );
    // A dropped byte reads as a corrupt frame three layers up. The WS door
    // is a different path from the TCP one, so this is a real question again
    // and not an inherited answer.
    assert_eq!(
        console.matches("dropping unparseable").count(),
        0,
        "the guest lost bytes:\n{console}"
    );
    // Board B was never uploaded to and must not have grown a project.
    assert!(
        !serve.console("c6-b").contains("Loading project"),
        "the walk landed on the wrong board, or on both"
    );

    serve.shutdown();

    // ---- gate 5 --------------------------------------------------------
    // The same state dir, a second server. PD8: blank → flash → loaded is a
    // sequence, and it only means something if the bytes survive.
    let flash = state.join("c6-a.flash.bin");
    assert!(flash.is_file(), "the board wrote its flash back");
    // And it is a chip with something ON it, not merely a file that exists.
    // This is the byte-level half of the survival claim, and it is here
    // rather than in the `flash` word because of what M5 made that word mean
    // — see the amended assertion below.
    let written = std::fs::read(&flash).expect("reading the board's flash file");
    assert!(
        written.iter().any(|&byte| byte != 0xff),
        "the write-back is an erased chip: {} bytes, every one of them 0xff",
        written.len()
    );

    // The console transcripts are the FIRST server's until the second one
    // flushes over them, and the first server's holds a blank board's boot —
    // `[FS] Mount failed (filesystem corrupt), formatting partition…`, which
    // is what a blank flash looks like. Reading that as the second boot's
    // would be reading the wrong run, so clear them and let the second
    // server write its own.
    for name in ["c6-a", "c6-b"] {
        let _ = std::fs::remove_file(state.join(format!("{name}.console.log")));
        let _ = std::fs::remove_file(state.join(format!("{name}.console-untaken.log")));
    }

    let again = Serve::start_in(&elf, &["c6-a", "c6-b"], &[], state.clone());
    // **Amended for plan two M5, deliberately.** This asserted `loaded`, and
    // it passed because the word was then "the flash FILE is non-empty" —
    // which a 4 MiB chip of `0xff` also satisfies. M5 made the word ask the
    // mask ROM's own question instead: is there an `esp_image_header_t`
    // magic byte at the reset vector? A `kind=elf` board's honest answer is
    // `blank` for as long as it lives — its firmware was loaded straight
    // into memory and never went through its chip, so the word was never
    // evidence that the walk's bytes survived, and reading it as such was
    // the coincidence M5's `flash_state()` header calls out by name.
    //
    // So the survival claim moves off the word and onto the two places that
    // can actually be asked: the bytes above, and the guest's own second
    // boot below. Both are stronger than what the word ever said; what
    // remains here is the word itself, still pinned, so a regression in it
    // is still a failure.
    assert_eq!(
        again.board("c6-a")["flash"],
        "blank",
        "a kind=elf board's CHIP never holds a bootable image, whatever its \
         data partitions hold for the guest"
    );
    let boot = again.wait_for_console("c6-a", "Boot:");
    // The milestone brief expected `Boot: found 1 entries in /projects`. The
    // walk's `loadProject` also sets the startup project (`load_project:
    // startup_project set to Basic` is in the first server's console), and
    // `fw_esp32_common::boot` takes the configured path first and never
    // scans — so this is the line, and both are "it found it".
    assert!(
        boot.contains("Boot: found configured startup project: Basic")
            || boot.contains("Boot: found 1 entries in /projects"),
        "the second boot did not find the project the walk uploaded:\n{boot}"
    );
    // A board that reformatted would have lost it, and would say so by name.
    assert!(
        !boot.contains("formatting partition"),
        "the second boot reformatted the flash — the write-back is not whole:\n{boot}"
    );
    assert!(
        !boot.contains("failed to list /projects"),
        "the second boot found no /projects at all:\n{boot}"
    );
}

/// The project the walk was captured against, materialised out of git
/// (`walks/README.md`), or `None` with an honest skip.
fn walk_project() -> Option<PathBuf> {
    let root = workspace_root()?;
    let into = root.join("target").join("walk-projects");
    let project = into.join("examples").join("basic");
    if project.join("project.json").is_file() {
        return Some(project);
    }
    if std::fs::create_dir_all(&into).is_err() {
        skip("emu_serve_walk", "could not create target/walk-projects");
        return None;
    }
    let archive = Command::new("git")
        .args(["archive", WALK_COMMIT, "examples/basic"])
        .current_dir(&root)
        .output();
    let Ok(archive) = archive else {
        skip("emu_serve_walk", "git archive is not runnable here");
        return None;
    };
    if !archive.status.success() {
        skip(
            "emu_serve_walk",
            &format!(
                "`git archive {WALK_COMMIT} examples/basic` failed: {}",
                String::from_utf8_lossy(&archive.stderr).trim()
            ),
        );
        return None;
    }
    let mut tar = match Command::new("tar")
        .args(["-x", "-C"])
        .arg(&into)
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            skip("emu_serve_walk", "tar is not runnable here");
            return None;
        }
    };
    {
        use std::io::Write;
        tar.stdin
            .as_mut()
            .expect("piped")
            .write_all(&archive.stdout)
            .expect("writing the archive");
    }
    if !tar.wait().expect("tar").success() {
        skip("emu_serve_walk", "unpacking the walk project failed");
        return None;
    }
    project.join("project.json").is_file().then_some(project)
}

fn workspace_root() -> Option<PathBuf> {
    let mut dir: &Path = Path::new(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("justfile").is_file() && dir.join("Cargo.toml").is_file() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// The gate asserts figures, never a cycle and never a duration. This checks
/// itself the way `emu_serve_door.rs` does.
#[test]
fn the_gate_asserts_figures_never_cycles() {
    let source = include_str!("emu_serve_walk.rs");
    for (n, line) in source.lines().enumerate() {
        let line = line.trim();
        if !line.starts_with("assert") {
            continue;
        }
        for forbidden in ["cyc=", " us=", "elapsed", "Duration", "millis"] {
            assert!(
                !line.contains(forbidden),
                "line {}: a socket is not deterministic, so `{forbidden}` is not something to \
                 assert: {line}",
                n + 1
            );
        }
    }
}
