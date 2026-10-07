//! Over-the-air updates on `lp-emu:esp32c6`, scenario by scenario (OTA plan
//! Part B, P08; `just test-emu-c6-ota`).
//!
//! The board is the shipped split image booted from the reset vector over a
//! flash file it keeps writing (`kind=rom-up`, `reboot_on_reset`), hosted in
//! this process on its USB link ([`EmuLinkHost`]), with `lp-cli`'s own
//! update host ([`OtaHost`], `lpa-update`'s driver) on channel 3 — or, for
//! the refusals, raw channel-3 messages. Every assertion is on the board's
//! own words (its `[OTA]`/`[CORE]` lines and its channel-3 messages), its
//! flash, or the frames decoded off its pad. Nothing asserts on time.
//!
//! The images (built by the recipe, named by `LP_OTA_IMAGES`, each a
//! `scripts/ota/build-image.sh` output: `merged.bin`, `split.json`, `ota/`):
//!
//! - `x` — the packaged split image, app version `a0a0a0a0`;
//! - `y` — the same tree at `b1b1b1b1`;
//! - `y-dies` — `y` with `fixture-trial-dies`;
//! - `x-untrusted` — `x` with `fixture-usb-untrusted`.
//!
//! A **power cut** after request N is the flash as it stood when the host had
//! answered the board's Nth request: one update is run and the flash
//! snapshotted at every chosen N (the same bytes N separate cut runs would
//! leave, the run being deterministic), then each snapshot boots cold and
//! recovers under a host offering Y again. Not in CI (DM26).

use std::path::{Path, PathBuf};
use std::process::Command;

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost, EmuUsbBoard};
use lp_cli::commands::ota_host::{OtaArgs, OtaHost};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{AppSource, BootMode, Esp32C6Builder, UsbHost};
use lpa_update::{Finish, StopReason, board_matches_release};
use lpc_firmware_release::OtaManifest;
use lpc_update::{BoardManifest, BoardMessage, BoardState, Refusal};

/// The emulated host's link nonce (any fixed value: runs repeat).
const NONCE: u32 = 0x07A0_0001;
/// The pad the walk's project (`projects/test/shader-oracle`) drives.
const PAD: u8 = 18;
/// The engine-less board's sector to erase: the engine header's.
const SECTOR: usize = 0x1000;
/// Everything below `lpfs` (0x350000 in the C6 table): what a refusal must
/// leave byte-identical.
const BELOW_LPFS: usize = 0x35_0000;
/// Steps (250 µs of emulated time each) a whole update may take.
const UPDATE_STEPS: u64 = 1_600_000;

// --- U1, U13, U17: X -> Y raw, the read-back, the identity ---------------------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota`"]
fn u01_x_to_y_raw_with_a_read_back_and_the_identity() {
    let (Some(x), Some(y)) = (image("x"), image("y")) else {
        return;
    };
    let cache = tempfile::tempdir().unwrap();
    let mut host = hosted(
        x.merged(),
        Some(offer(&y, |a| {
            a.ota_no_z = true;
            a.ota_cache = Some(cache.path().to_path_buf());
        })),
    );
    let finish = run_to_done(&mut host, None);
    assert_eq!(finish, Finish::UpToDate, "{}", tail(&host));
    let ota = host.ota.as_ref().unwrap();
    assert_eq!(ota.served().chunks_encoded, 0, "raw only");
    assert!(console_has(&host, "[OTA] core verified, committing"));
    assert!(console_has(&host, "[OTA] engine verified, committing"));
    expect_last_core(&host, &y, true);

    // U13: the backup read back before the update is X's engine.bin.
    let kept = std::fs::read(
        cache
            .path()
            .join(format!("{}.bin", x.manifest.engine.sha256)),
    )
    .expect("the read-back landed in the cache");
    assert_eq!(
        kept,
        x.engine(),
        "the read-back is byte-identical to engine.bin"
    );

    // U17: the board said exactly X's identity before, and Y's after.
    let first = &ota.manifests[0];
    assert_eq!(first.state, BoardState::Running);
    identity(first, &x.manifest, "X running");
    let last = ota.manifests.last().unwrap();
    assert_eq!(last.state, BoardState::Running);
    identity(last, &y.manifest, "Y running");
    // The hello's `firmware` is the same manifest `M` is.
    let hello = last_hello_firmware(&host).expect("Y's hello carries firmware");
    assert_eq!(
        (
            &hello.build_id,
            &hello.core_sha256,
            &hello.engine_sha256,
            hello.state
        ),
        (
            &last.build_id,
            &last.core_sha256,
            &last.engine_sha256,
            last.state
        ),
        "the hello's firmware is M"
    );
    report(
        "U1/U13/U17",
        &format!(
            "X->Y raw: {}; read-back = X engine.bin; M = ota-manifest.json for X and Y",
            ota.summary()
        ),
    );
}

// --- U2, U16: X -> Y with Z, and the light ----------------------------------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota`"]
fn u02_x_to_y_with_z_and_the_light() {
    let (Some(x), Some(y)) = (image("x"), image("y")) else {
        return;
    };
    let chip = with_project(&x);
    let mut host = hosted(chip, Some(offer(&y, |_| {})));
    let mut lit = Vec::new();
    let finish = run_to_done(&mut host, Some(&mut lit));
    assert_eq!(finish, Finish::UpToDate, "{}", tail(&host));
    let served = host.ota.as_ref().unwrap().served();
    assert!(served.chunks_encoded > 0, "chunks went as Z");
    expect_last_core(&host, &y, true);
    // U16: dark yellow while updating, dark red while it needs its engine,
    // on the pad the project's strip is on (GRB on the wire).
    assert!(lit.contains(&[0x10, 0x18, 0x00]), "dark yellow: {lit:02x?}");
    assert!(lit.contains(&[0x00, 0x18, 0x00]), "dark red: {lit:02x?}");
    report(
        "U2/U16",
        &format!(
            "X->Y Z: {} Z chunks / {} B, {} raw / {} B; the pad lit {lit:02x?}",
            served.chunks_encoded, served.bytes_encoded, served.chunks_raw, served.bytes_raw
        ),
    );
}

// --- U3, U4: the cut sweeps -------------------------------------------------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota`"]
fn u03_power_cuts_across_a_raw_update_converge_and_resume() {
    cut_sweep("U3", true);
}

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota`"]
fn u04_power_cuts_across_a_z_update_converge_and_resume() {
    cut_sweep("U4", false);
}

fn cut_sweep(id: &str, raw: bool) {
    let (Some(x), Some(y)) = (image("x"), image("y")) else {
        return;
    };
    let core_chunks = y.manifest.core.length.div_ceil(4096) as u32;
    let engine_chunks = y.manifest.engine.length.div_ceil(4096) as u32;
    let total = core_chunks + engine_chunks;
    let cuts = cut_points(core_chunks, total);
    // One update, the flash snapshotted after each chosen request.
    let mut host = hosted(x.merged(), Some(offer(&y, |a| a.ota_no_z = raw)));
    let mut snaps: Vec<(u32, &'static str, Vec<u8>)> = Vec::new();
    let mut next = 0;
    for _ in 0..UPDATE_STEPS {
        host.step().expect("the run");
        let ota = host.ota.as_ref().unwrap();
        while next < cuts.len() && ota.requests >= cuts[next] {
            let piece = if cuts[next] <= core_chunks {
                "core"
            } else {
                "engine"
            };
            snaps.push((cuts[next], piece, flash_of(&host)));
            next += 1;
        }
        if ota.done() {
            break;
        }
    }
    assert_eq!(snaps.len(), cuts.len(), "every cut point was reached");
    // Each snapshot boots cold and recovers, in parallel.
    let y_dir = y.dir.clone();
    let x_build = x.build_id.clone();
    let y_build = y.build_id.clone();
    let rows: Vec<String> = std::thread::scope(|s| {
        let handles: Vec<_> = snaps
            .chunks(snaps.len().div_ceil(threads()))
            .map(|batch| {
                let y_dir = y_dir.clone();
                let (x_build, y_build) = (x_build.clone(), y_build.clone());
                s.spawn(move || {
                    batch
                        .iter()
                        .map(|(n, piece, flash)| {
                            recover(*n, piece, flash.clone(), &y_dir, raw, &x_build, &y_build)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    for row in &rows {
        println!("{id} {row}");
    }
    let failed: Vec<&String> = rows.iter().filter(|r| r.starts_with("FAIL")).collect();
    assert!(failed.is_empty(), "{failed:#?}");
    report(
        id,
        &format!(
            "{} cuts over {total} requests ({core_chunks} core + {engine_chunks} engine), {}: all converge to Y",
            rows.len(),
            if raw { "D" } else { "Z" }
        ),
    );
}

/// One cut's recovery: boot `flash` cold, offer Y, run to done. A row.
fn recover(
    n: u32,
    piece: &str,
    flash: Vec<u8>,
    y_dir: &Path,
    raw: bool,
    x_build: &str,
    y_build: &str,
) -> String {
    let y = load(y_dir).unwrap();
    let mut host = hosted(flash, Some(offer(&y, |a| a.ota_no_z = raw)));
    let finish = run_to_done(&mut host, None);
    let console = host.console().join("\n");
    let first = console
        .lines()
        .find(|l| l.contains("[CORE] core @"))
        .map(|l| {
            if l.contains(x_build) {
                "X"
            } else if l.contains(y_build) {
                "Y"
            } else {
                "?"
            }
        })
        .unwrap_or("none");
    let reachable = console.contains("[OTA] core-only:") || console.contains("\"hello\":{");
    let resumed = console
        .lines()
        .find(|l| l.contains("[OTA] resuming"))
        .map(|l| l[l.find("resuming").unwrap()..].to_string())
        .unwrap_or_default();
    let ota = host.ota.as_ref().unwrap();
    let served = ota.served();
    let served_bytes = served.bytes_raw + served.bytes_encoded;
    let last_y = last_core_build(&console) == Some(y_build.to_string()) && engine_entered(&console);
    let ok = finish == Finish::UpToDate && last_y && reachable;
    format!(
        "{}\tcut-after={n}\tpiece={piece}\tfirst-boot={first}{}\t{resumed}\tserved-after={} req / {served_bytes} B",
        if ok { "PASS" } else { "FAIL" },
        if reachable {
            " reachable"
        } else {
            " NOT REACHABLE"
        },
        ota.requests,
    )
}

/// ≥ 30 cuts: dense around the core's last chunk, the engine's first chunk
/// and the header (the engine's last request), spread elsewhere.
fn cut_points(core_chunks: u32, total: u32) -> Vec<u32> {
    let mut v: Vec<u32> = vec![1, 2, 3];
    v.extend((1..=8).map(|k| k * core_chunks / 9));
    v.extend(core_chunks.saturating_sub(2)..=core_chunks + 3);
    v.extend((1..=8).map(|k| core_chunks + k * (total - core_chunks) / 9));
    v.extend(total.saturating_sub(3)..=total);
    v.extend([core_chunks / 2 + 1, core_chunks + 10, total - 10]);
    v.sort_unstable();
    v.dedup();
    v.retain(|n| *n >= 1 && *n <= total);
    assert!(v.len() >= 30, "{v:?}");
    v
}

// --- U5: E2, a pending core transfer and a host holding only X ------------------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota`"]
fn u05_a_host_holding_only_the_old_engine_heals_and_cancels_the_pending_update() {
    let (Some(x), Some(y)) = (image("x"), image("y")) else {
        return;
    };
    // The cut: right after the running engine handed over (pending record,
    // header erased) and core-only reported it, before any chunk.
    let mut host = hosted(x.merged(), Some(offer(&y, |a| a.ota_no_z = true)));
    let mut snap = None;
    for _ in 0..UPDATE_STEPS {
        host.step().expect("the run");
        let ota = host.ota.as_ref().unwrap();
        if ota.manifests.last().is_some_and(|m| {
            m.state == BoardState::Updating && m.transfer.is_some_and(|t| t.done == 0)
        }) {
            snap = Some(flash_of(&host));
            break;
        }
    }
    let snap = snap.expect("core-only reported the pending update");
    let mut host = hosted(snap, Some(offer(&x, |a| a.ota_heal_only = true)));
    let mut decisions = Vec::new();
    for _ in 0..UPDATE_STEPS {
        host.step().expect("the run");
        let console = host.console().join("\n");
        if console.contains("decided: Heal") && !decisions.contains(&"heal") {
            decisions.push("heal");
        }
        if host
            .ota
            .as_ref()
            .unwrap()
            .manifests
            .last()
            .is_some_and(|m| m.state == BoardState::Running)
        {
            break;
        }
    }
    let ota = host.ota.as_ref().unwrap();
    let last = ota.manifests.last().unwrap();
    assert_eq!(last.state, BoardState::Running, "{}", tail(&host));
    assert_eq!(last.build_id, x.build_id, "X runs again");
    assert!(last.transfer.is_none(), "the pending transfer is gone");
    assert!(decisions.contains(&"heal"), "{}", tail(&host));
    report(
        "U5",
        "pending X->Y, host holds only X: healed X, pending transfer cancelled, X runs",
    );
}

// --- U6, U9: an engine-less board heals with no login --------------------------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota`"]
fn u06_an_engine_less_board_heals_from_the_cache_with_no_login() {
    let (Some(x), Some(y)) = (image("x"), image("y")) else {
        return;
    };
    let chip = engine_less(&with_project(&x), &x);
    heal_from_cache("U6", chip, &x, &y, true);
}

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota`"]
fn u09_an_untrusted_engine_less_board_heals_with_no_password() {
    let (Some(xu), Some(y)) = (image("x-untrusted"), image("y")) else {
        return;
    };
    let chip = engine_less(&with_store(&xu), &xu);
    heal_from_cache("U9", chip, &xu, &y, false);
}

fn heal_from_cache(id: &str, chip: Vec<u8>, x: &Image, y: &Image, light: bool) {
    let cache = tempfile::tempdir().unwrap();
    std::fs::write(
        cache
            .path()
            .join(format!("{}.bin", x.manifest.engine.sha256)),
        x.engine(),
    )
    .unwrap();
    let mut host = hosted(
        chip,
        Some(offer(y, |a| {
            a.ota_heal_only = true;
            a.ota_cache = Some(cache.path().to_path_buf());
        })),
    );
    let mut lit = Vec::new();
    for _ in 0..UPDATE_STEPS {
        host.step().expect("the run");
        collect_lit(&host, &mut lit);
        let ota = host.ota.as_ref().unwrap();
        if ota
            .manifests
            .last()
            .is_some_and(|m| m.state == BoardState::Running)
        {
            break;
        }
    }
    let ota = host.ota.as_ref().unwrap();
    let first = &ota.manifests[0];
    assert_eq!(first.state, BoardState::NeedsEngine);
    assert_eq!(
        first.engine_len, None,
        "an engine-less core cannot know its engine's length"
    );
    // U17's engine-less case: every identity field but engineLen still holds.
    identity(first, &x.manifest, "engine-less X");
    let last = ota.manifests.last().unwrap();
    assert_eq!(
        (last.state, &last.build_id),
        (BoardState::Running, &x.build_id),
        "{}",
        tail(&host)
    );
    assert!(
        ota.refusals.is_empty(),
        "no login asked: {:?}",
        ota.refusals
    );
    if light {
        assert!(
            lit.contains(&[0x00, 0x18, 0x00]),
            "dark red while it needed its engine: {lit:02x?}"
        );
    }
    report(
        id,
        &format!("engine-less -> healed from the cache, no login; lit {lit:02x?}"),
    );
}

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota`"]
fn u07_an_engine_that_keeps_crashing() {
    println!(
        "SKIP U7: an engine that keeps crashing needs a fixture or a seeded recovery ledger (RTC \
         RAM, cleared on every power-on boot this harness makes) — M2's S10 skips for the same \
         reason. The board side is lpc-update's `board_access.rs` (the `engine-crashing` state) \
         and the host side lpa-update's `decide.rs` (`ReportCrashing`, never an automatic heal)."
    );
}

// --- U8: a core install needs edit ------------------------------------------------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota`"]
fn u08_a_core_install_needs_edit_and_a_core_side_login_grants_it() {
    let (Some(xu), Some(y)) = (image("x-untrusted"), image("y")) else {
        return;
    };
    let chip = with_store(&xu);
    // (a) The running engine, USB untrusted, open to nobody: refused N/A
    // before anything is erased; its channel 3 takes no login (the
    // server's, channel 1, is the engine's), so the host stops there.
    let mut host = hosted(chip.clone(), Some(offer(&y, |a| a.ota_no_z = true)));
    let finish = run_to_done(&mut host, None);
    assert_eq!(
        finish,
        Finish::Stopped(StopReason::NeedsEngineLogin),
        "{}",
        tail(&host)
    );
    // USB pulls the read-back four ahead: every `G` already in flight is
    // refused the same way before the host stops.
    let refusals = &host.ota.as_ref().unwrap().refusals;
    assert!(
        !refusals.is_empty() && refusals.iter().all(|r| *r == b'A'),
        "{refusals:?}"
    );
    untouched(&flash_of(&host), &chip, "nothing written");

    // (b) Core-only (engine-less), no password: N/A, nothing written.
    let bare = engine_less(&chip, &xu);
    let mut host = hosted(bare.clone(), Some(offer(&y, |a| a.ota_no_z = true)));
    let finish = run_to_done(&mut host, None);
    assert_eq!(
        finish,
        Finish::Stopped(StopReason::NoCredentials),
        "{}",
        tail(&host)
    );
    untouched(&flash_of(&host), &bare, "nothing written");

    // (c) A wrong password: refused.
    let mut host = hosted(
        bare.clone(),
        Some(offer(&y, |a| {
            a.ota_no_z = true;
            a.ota_password = Some("not-it".into());
        })),
    );
    let finish = run_to_done(&mut host, None);
    assert_eq!(
        finish,
        Finish::Stopped(StopReason::LoginRefused),
        "{}",
        tail(&host)
    );
    untouched(&flash_of(&host), &bare, "nothing written");

    // (d) The right password: a core-side login, then the update.
    let mut host = hosted(
        bare,
        Some(offer(&y, |a| {
            a.ota_no_z = true;
            a.ota_password = Some(PASSWORD.into());
        })),
    );
    let finish = run_to_done(&mut host, None);
    assert_eq!(finish, Finish::UpToDate, "{}", tail(&host));
    expect_last_core(&host, &y, true);
    report(
        "U8",
        "untrusted USB, open nobody: engine N/A (flash untouched); core-only N/A without a \
         password, refused with a wrong one, a core-side login with the right one -> Y runs",
    );
}

// --- U10, U11, U18: refusals before anything is erased ----------------------------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota`"]
fn u10_u11_u18_refusals_come_before_any_erase() {
    let (Some(x), Some(y)) = (image("x"), image("y")) else {
        return;
    };
    let chip = x.merged();
    let mut host = hosted(chip.clone(), None);
    wait_line(&mut host, "\"hello\":{");
    let base = y_offer(&y);
    let cases: Vec<(&str, lpc_update::Offer, Refusal)> = vec![
        (
            "U10 too big",
            lpc_update::Offer {
                core_len: 3_000_000,
                ..base.clone()
            },
            Refusal::DoesNotFit { need: 0, room: 0 },
        ),
        (
            "U11 another chip",
            lpc_update::Offer {
                chip: 2,
                ..base.clone()
            },
            Refusal::Incompatible {
                what: lpc_update::Mismatch::Chip,
                have: 0,
                need: 0,
            },
        ),
        (
            "U11 another layout",
            lpc_update::Offer {
                layout: 2,
                ..base.clone()
            },
            Refusal::Incompatible {
                what: lpc_update::Mismatch::Layout,
                have: 0,
                need: 0,
            },
        ),
        (
            "U11 loader 9",
            lpc_update::Offer {
                min_loader: 9,
                ..base.clone()
            },
            Refusal::Incompatible {
                what: lpc_update::Mismatch::Loader,
                have: 0,
                need: 0,
            },
        ),
        (
            "U18 a must-understand flag",
            lpc_update::Offer {
                flags: 0x40,
                ..base.clone()
            },
            Refusal::Incompatible {
                what: lpc_update::Mismatch::Flags,
                have: 0,
                need: 0,
            },
        ),
    ];
    let mut lines = Vec::new();
    for (name, offer, want) in cases {
        let got = exchange(&mut host, &offer.encode());
        let refusal =
            refusal_of(&got).unwrap_or_else(|| panic!("{name}: no refusal in {got:02x?}"));
        assert_eq!(
            std::mem::discriminant(&refusal),
            std::mem::discriminant(&want),
            "{name}: {refusal:?}"
        );
        if let (Refusal::Incompatible { what, .. }, Refusal::Incompatible { what: want, .. }) =
            (&refusal, &want)
        {
            assert_eq!(what, want, "{name}");
        }
        lines.push(format!("{name}: {refusal:?}"));
    }
    // U18: an unknown host message is `N`/`U` with its type byte.
    let got = exchange(&mut host, b"X\x01\x02");
    assert_eq!(
        refusal_of(&got),
        Some(Refusal::UnknownMessage { ty: b'X' }),
        "{got:02x?}"
    );
    lines.push("U18 an unknown message: UnknownMessage { ty: 'X' }".into());
    untouched(
        &flash_of(&host),
        &chip,
        "every refusal came before any erase",
    );
    // U18: an unknown LOW flag bit is ignored: that offer proceeds (the
    // running engine hands over and resets).
    let mut low = y_offer(&y);
    low.flags = 0x04;
    host.port.send_update(&low.encode()).unwrap();
    assert!(
        wait_line(&mut host, "[OTA] core-only: updating").is_some(),
        "{}",
        tail(&host)
    );
    lines.push("U18 an unknown low flag bit: the offer proceeded".into());
    for l in &lines {
        println!("U10/U11/U18 {l}");
    }
    report(
        "U10/U11/U18",
        "every refusal before any erase; flash below lpfs byte-identical but for the boot records' marks",
    );
}

// --- U12: a trial that dies is refused from then on ------------------------------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota`"]
fn u12_a_build_that_dies_on_trial_rolls_back_and_is_refused() {
    let (Some(x), Some(yd)) = (image("x"), image("y-dies")) else {
        return;
    };
    let cache = tempfile::tempdir().unwrap();
    let mut host = hosted(
        x.merged(),
        Some(offer(&yd, |a| {
            a.ota_no_z = true;
            a.ota_cache = Some(cache.path().to_path_buf());
        })),
    );
    let finish = run_to_done(&mut host, None);
    let failed = lp_bootctl::build_hash(yd.build_id.as_bytes());
    assert_eq!(
        finish,
        Finish::Stopped(StopReason::Decision(lpa_update::Decision::RefusedBuild {
            build: failed
        })),
        "{}",
        tail(&host)
    );
    let ota = host.ota.as_ref().unwrap();
    let last = ota.manifests.last().unwrap();
    assert_eq!(last.refused_build, Some(failed), "M names the failed build");
    // Rolled back to X, whose engine the new core overwrote: an `Install`
    // of a refused build stops there (no intent overrides a refused build,
    // lpa-update's `decide_for_intent`), and the heal is the next host's.
    assert_eq!(
        (last.state, &last.build_id),
        (BoardState::NeedsEngine, &x.build_id)
    );
    assert!(console_has(&host, "fixture-trial-dies"));
    // The next host (no press: `Auto`) heals X from the read-back the first
    // one kept.
    host.ota = OtaHost::from_args(&offer(&yd, |a| {
        a.ota_heal_only = true;
        a.ota_cache = Some(cache.path().to_path_buf());
    }))
    .unwrap();
    let now_ms = host.now_us() / 1_000;
    host.ota.as_mut().unwrap().link_up(now_ms);
    let finish = run_to_done(&mut host, None);
    assert_eq!(
        finish,
        Finish::Stopped(StopReason::Decision(lpa_update::Decision::RefusedBuild {
            build: failed
        })),
        "{}",
        tail(&host)
    );
    let last = host.ota.as_ref().unwrap().manifests.last().unwrap().clone();
    assert_eq!(
        (last.state, &last.build_id),
        (BoardState::Running, &x.build_id),
        "X healed and runs"
    );
    assert_eq!(last.refused_build, Some(failed), "still named once X runs");
    // The next offer of it is refused N/F.
    host.ota = None;
    let got = exchange(&mut host, &y_offer(&yd).encode());
    assert_eq!(
        refusal_of(&got),
        Some(Refusal::FailedBuild { build_hash: failed }),
        "{got:02x?}"
    );
    report(
        "U12",
        &format!(
            "Y-dies rolled back to X (engine-less), M.refusedBuild = {failed:#010x}; the next \
             host healed X from the read-back; the next offer of Y-dies N/F"
        ),
    );
}

// --- U14: a host that goes away mid-core, and a new one that resumes ------------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota`"]
fn u14_a_host_that_disconnects_mid_core_is_resumed_by_the_next() {
    let (Some(x), Some(y)) = (image("x"), image("y")) else {
        return;
    };
    let mut host = hosted(x.merged(), Some(offer(&y, |a| a.ota_no_z = true)));
    for _ in 0..UPDATE_STEPS {
        host.step().expect("the run");
        if host.ota.as_ref().unwrap().requests >= 120 {
            break;
        }
    }
    // The host goes: nothing services the link for 20 emulated seconds.
    host.board
        .run_for_us(20_000_000)
        .expect("the board runs alone");
    // A new host, a fresh link session (a new nonce), the same offer.
    let EmuLinkHost { board, .. } = host;
    let mut host = EmuLinkHost::new(board, NONCE ^ 0xFFFF, true);
    host.ota = OtaHost::from_args(&offer(&y, |a| a.ota_no_z = true)).unwrap();
    let finish = run_to_done(&mut host, None);
    assert_eq!(finish, Finish::UpToDate, "{}", tail(&host));
    let resumed = host
        .console()
        .iter()
        .find(|l| l.contains("[OTA] resuming core at"))
        .cloned()
        .expect("the second host resumed the core");
    report(
        "U14",
        &format!("host 1 gone after 120 requests, 20 s silence; host 2: `{resumed}`"),
    );
}

// --- U15: a corrupted raw chunk restarts its piece -------------------------------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota`"]
fn u15_a_corrupted_raw_chunk_fails_its_piece_which_is_sent_again() {
    let (Some(x), Some(y)) = (image("x"), image("y")) else {
        return;
    };
    let mut host = hosted(
        x.merged(),
        Some(offer(&y, |a| {
            a.ota_no_z = true;
            a.ota_corrupt = Some("E:0x40000".into());
        })),
    );
    let finish = run_to_done(&mut host, None);
    assert_eq!(finish, Finish::UpToDate, "{}", tail(&host));
    assert!(
        host.ota.as_ref().unwrap().refusals.contains(&b'H'),
        "N/H at the piece's end"
    );
    assert!(console_has(&host, "corrupted the raw chunk"));
    expect_last_core(&host, &y, true);
    report(
        "U15",
        "a flipped byte in engine chunk 0x40000: N/H at the piece end, the engine sent again, Y runs",
    );
}

// --- the images -------------------------------------------------------------------------------

#[derive(Clone)]
struct Image {
    dir: PathBuf,
    manifest: OtaManifest,
    build_id: String,
    engine_offset: usize,
}

impl Image {
    fn merged(&self) -> Vec<u8> {
        std::fs::read(self.dir.join("merged.bin")).expect("merged.bin")
    }

    fn engine(&self) -> Vec<u8> {
        std::fs::read(self.dir.join("ota/engine.bin")).expect("engine.bin")
    }
}

fn image(name: &str) -> Option<Image> {
    let Some(root) = std::env::var_os("LP_OTA_IMAGES") else {
        println!("SKIP: LP_OTA_IMAGES is not set — `just test-emu-c6-ota` builds them");
        return None;
    };
    Some(load(&PathBuf::from(root).join(name)).expect("an OTA image"))
}

fn load(dir: &Path) -> Option<Image> {
    let split: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("split.json")).ok()?).ok()?;
    let manifest =
        OtaManifest::parse_valid(&std::fs::read(dir.join("ota/ota-manifest.json")).ok()?).ok()?;
    Some(Image {
        dir: dir.to_path_buf(),
        build_id: split["buildId"].as_str()?.to_string(),
        engine_offset: split["engine"]["offset"].as_u64()? as usize,
        manifest,
    })
}

/// The `--ota-*` flags for offering `img`, edited by `edit`.
fn offer(img: &Image, edit: impl FnOnce(&mut OtaArgs)) -> OtaArgs {
    let mut args = OtaArgs {
        ota_offer: Some(img.dir.join("ota")),
        ..OtaArgs::default()
    };
    edit(&mut args);
    args
}

/// `img`'s offer, as its host would send it.
fn y_offer(img: &Image) -> lpc_update::Offer {
    let (_, build) =
        lp_cli::commands::ota_host::ota_offer_dir::load_offer(&img.dir.join("ota"), true).unwrap();
    build.offer()
}

/// `chip` with its engine header sector erased: an engine-less board.
fn engine_less(chip: &[u8], img: &Image) -> Vec<u8> {
    let mut chip = chip.to_vec();
    chip[img.engine_offset..img.engine_offset + SECTOR].fill(0xff);
    chip
}

/// `img`'s chip after the walk's project was uploaded and ran: lpfs holds
/// it, and the engine recorded the strip its light may drive.
fn with_project(img: &Image) -> Vec<u8> {
    seeded(
        img,
        &["--upload", "projects/test/shader-oracle"],
        "status light: GPIO",
    )
}

/// The password U8/U9's store holds.
const PASSWORD: &str = "ota-u8";

/// `img`'s chip with a device store open to nobody and one edit password
/// (set over channel 1, which the engine still trusts on USB).
fn with_store(img: &Image) -> Vec<u8> {
    let entry = lpc_access::SecretEntry::from_password(
        "ota",
        lpc_access::Tier::Edit,
        PASSWORD.as_bytes(),
        [7u8; lpc_access::SALT_BYTES],
        1000,
    );
    let add = serde_json::to_string(&lpc_wire::ClientRequest::AccessAdd { entry }).unwrap();
    let close = serde_json::to_string(&lpc_wire::ClientRequest::AccessSetSwitches {
        ble_enabled: None,
        open: Some(lpc_access::OpenTo::Nobody),
    })
    .unwrap();
    seeded(
        img,
        &["--request", &add, "--request", &close],
        "\"accessList\"",
    )
}

/// Boot `img` once with `lp-cli emu run --host-link <extra>` over a flash
/// file and return the flash as it was left.
fn seeded(img: &Image, extra: &[&str], exit_on: &str) -> Vec<u8> {
    let tmp = tempfile::tempdir().unwrap();
    let flash = tmp.path().join("chip.bin");
    std::fs::copy(img.dir.join("merged.bin"), &flash).unwrap();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
        .current_dir(repo)
        .args(["emu", "run", "--rom-up-flash"])
        .arg(&flash)
        .args(["--host-link", "--timeout", "30s", "--exit-on", exit_on])
        .args(extra)
        .output()
        .expect("lp-cli emu run");
    assert!(
        out.status.success(),
        "seeding failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read(&flash).unwrap()
}

// --- the machine ------------------------------------------------------------------------------

fn hosted(chip: Vec<u8>, ota: Option<OtaArgs>) -> EmuLinkHost<C6Board> {
    let len = chip.len() as u32;
    let machine = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .app(AppSource::None)
        .flash(FlashBacking::Bytes(chip))
        .flash_len(len)
        .usb_host(UsbHost::Attached { draining: true })
        .reboot_on_reset(true)
        .usb_sj_queue_source()
        .build()
        .expect("the machine builds");
    let mut host = EmuLinkHost::new(C6Board::new(machine).expect("a hosted board"), NONCE, true);
    if let Some(args) = ota {
        host.ota = OtaHost::from_args(&args).expect("the offer loads");
    }
    host
}

/// Step until the update host ends; how it ended. `lit` collects every
/// solid colour decoded off the pad as it goes.
fn run_to_done(host: &mut EmuLinkHost<C6Board>, mut lit: Option<&mut Vec<[u8; 3]>>) -> Finish {
    for _ in 0..UPDATE_STEPS {
        host.step().expect("the run");
        if let Some(lit) = lit.as_deref_mut() {
            collect_lit(host, lit);
        }
        if let Some(finish) = host.ota.as_ref().and_then(|o| o.finish.clone()) {
            return finish;
        }
    }
    panic!("the update never ended:\n{}", tail(host));
}

/// Every solid colour (one wire triple repeated) decoded on the pad so far.
fn collect_lit(host: &EmuLinkHost<C6Board>, lit: &mut Vec<[u8; 3]>) {
    for frame in host.board.machine.frames(PAD) {
        let w = &frame.wire;
        if w.len() >= 3 && w.len() % 3 == 0 && w.chunks(3).all(|c| c == &w[..3]) {
            let c = [w[0], w[1], w[2]];
            if !lit.contains(&c) {
                lit.push(c);
            }
        }
    }
}

/// Send one raw channel-3 message (no update host) and gather the board's
/// channel-3 answers for a while.
fn exchange(host: &mut EmuLinkHost<C6Board>, msg: &[u8]) -> Vec<Vec<u8>> {
    host.port.send_update(msg).unwrap();
    let mut got = Vec::new();
    for _ in 0..8_000 {
        host.step().expect("the run");
        while let Some(m) = host.port.poll_update() {
            got.push(m);
        }
        if got.iter().any(|m| m.first() == Some(&b'N')) {
            break;
        }
    }
    got
}

fn refusal_of(msgs: &[Vec<u8>]) -> Option<Refusal> {
    msgs.iter().find_map(|m| match BoardMessage::decode(m) {
        Ok(BoardMessage::Refusal(r)) => Some(r),
        _ => None,
    })
}

fn wait_line(host: &mut EmuLinkHost<C6Board>, needle: &str) -> Option<String> {
    host.wait_for_line(needle, 60_000_000).expect("the run")
}

fn flash_of(host: &EmuLinkHost<C6Board>) -> Vec<u8> {
    host.board.machine.flash().lock().unwrap().bytes().to_vec()
}

/// `after` is `before` below lpfs, but for the boot records' marks: a boot
/// may mark its record (the engine guard's `confirmed`, a trial's), and
/// nothing else may have been written.
fn untouched(after: &[u8], before: &[u8], what: &str) {
    const RECORDS: [usize; 2] = [0x1_6000, 0x1_7000];
    const RECORD_LEN: usize = lp_bootctl::BOOT_RECORD_LEN;
    let mut spans = vec![(0, RECORDS[0])];
    for r in RECORDS {
        spans.push((r, r + RECORD_LEN));
    }
    spans.push((RECORDS[1] + SECTOR, BELOW_LPFS));
    for (a, b) in spans {
        assert!(
            after[a..b] == before[a..b],
            "{what}: flash moved in {a:#x}..{b:#x}"
        );
    }
}

fn console_has(host: &EmuLinkHost<C6Board>, needle: &str) -> bool {
    host.console().iter().any(|l| l.contains(needle))
}

fn last_core_build(console: &str) -> Option<String> {
    let line = console
        .lines()
        .rev()
        .find(|l| l.contains("[CORE] core @"))?;
    let at = line.find(" build ")? + 7;
    Some(line[at..].split_whitespace().next()?.to_string())
}

fn engine_entered(console: &str) -> bool {
    console
        .lines()
        .rev()
        .find(|l| l.contains("[CORE] core @"))
        .is_some_and(|l| l.contains("· engine ") && l.contains(" B ·"))
}

/// The last core line names `img`'s build (and, with `engine`, its engine).
fn expect_last_core(host: &EmuLinkHost<C6Board>, img: &Image, engine: bool) {
    let console = host.console().join("\n");
    assert_eq!(
        last_core_build(&console),
        Some(img.build_id.clone()),
        "{}",
        tail(host)
    );
    if engine {
        assert!(engine_entered(&console), "{}", tail(host));
    }
}

/// The last hello's `firmware` block, read off the console.
fn last_hello_firmware(host: &EmuLinkHost<C6Board>) -> Option<BoardManifest> {
    let line = host
        .console()
        .iter()
        .rev()
        .find(|l| l.starts_with("M!") && l.contains("\"hello\":{"))?;
    let v: serde_json::Value = serde_json::from_str(&line[2..]).ok()?;
    serde_json::from_value(v["msg"]["hello"]["firmware"].clone()).ok()
}

/// U17: `board` reports exactly `release`'s identity.
fn identity(board: &BoardManifest, release: &OtaManifest, what: &str) {
    if let Err(m) = board_matches_release(board, release) {
        panic!("{what}: the board's M differs from ota-manifest.json: {m:#?}");
    }
}

fn tail(host: &EmuLinkHost<C6Board>) -> String {
    let c = host.console();
    c[c.len().saturating_sub(60)..].join("\n")
}

fn threads() -> usize {
    std::env::var("LP_OTA_THREADS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(6)
}

fn report(id: &str, what: &str) {
    println!("{id} PASS (lp-emu:esp32c6:t1): {what}");
}
