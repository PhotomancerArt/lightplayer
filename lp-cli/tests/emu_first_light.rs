//! Time to first light on the emulated C6 (plan
//! `lp2025/2026-10-16-0848-tree-store-adoption`, A1): power-on to the first
//! lit LED frame, and its parts, from a ROM-up boot of the SPLIT image — the
//! mask ROM, the IDF bootloader, the loader, the core and the engine — for
//! the shipped image (the tree store since the cutover), with no project,
//! `projects/test/basic` and a c40-sized project.
//!
//! The littlefs leg (A1 ran both) is gone with littlefs from this tree: no
//! split littlefs image is built from it any more, and a pinned plain one
//! would not be the same boot (no loader, no core hash). The littlefs
//! figures are A1's, and the before/after on silicon is the cutover's CX1
//! run (the release littlefs split image against this one).
//!
//! **A report, never a gate** (AGENTS.md: never gate on emulated time). Each
//! case prints two readings of the same boot:
//!
//! - the firmware's own `[perf] first light …` line (`fw-esp32c6`'s
//!   `first_light`): its parts, on the system timer — the emulator models no
//!   LP timer, so the line's figures are from reset, not from power-up;
//! - the first lit WS281x frame decoded off a pad (the `--dump-frames`
//!   record's `start_us`): when its first bit left the chip, in guest time.
//!
//! The only things asserted are that the boot printed its line and lit a
//! frame (a project) or said `none by` (no project) — a boot that never
//! lights is a broken boot, not a slow one.
//!
//! Emulated: `lp-emu:esp32c6:<grade>` (`LP_FIRST_LIGHT_GRADE`: `t1`, `t2`,
//! the default, `t3` or `t4`), ROM-up. Never hardware-validated: the silicon
//! figures are `docs/reports/2026-10-16-c6-time-to-first-light.md`'s.
//! `LP_FIRST_LIGHT_DIR` writes `emu-first-light-<grade>.json` there;
//! For a like-for-like reading of a silicon sitting: `LP_FIRST_LIGHT_MERGED`
//! (`label=merged.bin,…`) boots that sitting's own chips instead of this
//! tree's, `LP_FIRST_LIGHT_CASES` (`none`, `basic`, `c40`, `project:<dir>`)
//! picks the projects, and `LP_FIRST_LIGHT_LPFS` (`label=lpfs.bin,…`) also
//! runs each case on a chip whose `lpfs` starts as that store.
//! `LP_FIRST_LIGHT_NET=1` puts each board on a virtual LAN with one access
//! point (made-up test values) and saves that network before the measured
//! boots, so every boot joins it (the network seam, `net=lan`); the row's
//! project is then `<case>+net`, and the boot is stepped on to the board's
//! address. The join's key derivation runs in esp-radio's Wi-Fi task on
//! silicon and is not run under the seam, so this shows the boot's ORDER
//! with a network saved, not the join's CPU cost
//! (`docs/defects/2026-10-17-the-emulated-join-costs-no-cpu.md`).
//!
//! `#[ignore]`d: it needs the split build (`LP_EMU_BUILD_FW=1`, or
//! `LP_EMU_C6_SPLIT_<SLUG>`).

#[path = "support/editor_reads.rs"]
mod editor_reads;
#[path = "support/tree_store_board.rs"]
mod tree_store_board;

use editor_reads::block_on;
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp_common::seam::net::{
    LanConfig, LanDriver, SharedLan, VirtualAccessPoint, VirtualLan,
};
use lp_emu_esp_common::{ParticipantId, Strap};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::loader::ResetCause;
use lp_emu_esp32c6::machine::{AppSource, BootMode, Esp32C6Builder, TimeGrade, UsbHost};
use lp_emu_esp32c6::memmap::CYCLES_PER_US;
use lp_emu_esp32c6::test_support::{FwImage, split_image};
use lpa_client::LpClient;
use tree_store_board::{c40_files, project_files};

/// Emulated µs a boot may take to its first-light line (a wedge detector).
const LINE_BUDGET_US: u64 = 30_000_000;

/// The virtual LAN's network (`LP_FIRST_LIGHT_NET`): made-up test values.
const NET_SSID: &str = "lp-walk-net";
const NET_PASSWORD: &str = "correct-horse-42";

/// Power-on boots per case. The machine is deterministic but a boot draws
/// a link nonce and the RNG, so two show whether the figure moves.
const BOOTS: usize = 2;

#[test]
#[ignore = "needs the split fw-esp32c6 build; a report, not a gate (module docs)"]
fn time_to_first_light_on_the_emulated_c6() {
    let grade = std::env::var("LP_FIRST_LIGHT_GRADE")
        .ok()
        .filter(|g| !g.is_empty())
        .map_or(TimeGrade::T2, |g| {
            TimeGrade::parse(&g).expect("LP_FIRST_LIGHT_GRADE is t1, t2, t3 or t4")
        });
    let grade_name = format!("{grade:?}").to_lowercase();
    let net = std::env::var("LP_FIRST_LIGHT_NET").is_ok_and(|v| v == "1");
    let mut rows = Vec::new();
    // `LP_FIRST_LIGHT_MERGED` (`label=merged.bin`, comma-joined) boots those
    // chips instead of this tree's split builds — a silicon sitting's own
    // images, for a like-for-like reading. ROM-up needs no ELF.
    let mut chips: Vec<(String, Vec<u8>, Option<std::path::PathBuf>)> = Vec::new();
    if let Ok(list) = std::env::var("LP_FIRST_LIGHT_MERGED")
        && !list.is_empty()
    {
        for pair in list.split(',') {
            let (label, path) = pair.split_once('=').expect("label=path");
            chips.push((
                label.to_string(),
                std::fs::read(path).expect("the merged image"),
                None,
            ));
        }
    }
    // The label stays `fs-tree`, so the JSON rows read as A1's tree rows did.
    for (label, image) in [("fs-tree", FwImage::SHIPPED)] {
        if !chips.is_empty() {
            continue;
        }
        let split = match split_image(&image) {
            Ok(split) => split,
            Err(reason) => {
                eprintln!("emu_first_light {label}: skipped — {reason}");
                continue;
            }
        };
        let merged = std::fs::read(split.merged()).expect("the merged image");
        chips.push((label.to_string(), merged, Some(split.p2_elf())));
    }
    // `LP_FIRST_LIGHT_CASES` (`none`, `basic`, `c40`, `project:<dir>`,
    // comma-joined; unset: the first three).
    let cases = std::env::var("LP_FIRST_LIGHT_CASES")
        .ok()
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| "none,basic,c40".to_string());
    // `LP_FIRST_LIGHT_LPFS` (`label=lpfs.bin`, comma-joined): also run each
    // case on a chip whose `lpfs` starts as that store (written out, used,
    // full) rather than blank.
    let mut stores: Vec<(String, Option<Vec<u8>>)> = vec![("fresh".to_string(), None)];
    if let Ok(list) = std::env::var("LP_FIRST_LIGHT_LPFS")
        && !list.is_empty()
    {
        for pair in list.split(',') {
            let (label, path) = pair.split_once('=').expect("label=path");
            stores.push((
                label.to_string(),
                Some(std::fs::read(path).expect("the lpfs image")),
            ));
        }
    }
    for (label, merged, p2_elf) in &chips {
        let label = label.as_str();
        for (store, region) in &stores {
            for case in cases.split(',') {
                let (project, files) = match case {
                    "none" => ("none".to_string(), None),
                    "basic" => (
                        "basic".to_string(),
                        Some(project_files("projects/test/basic")),
                    ),
                    "c40" => ("c40".to_string(), Some(c40_files())),
                    other => {
                        let dir = other.strip_prefix("project:").expect("a known case");
                        (
                            dir.rsplit('/').next().unwrap().to_string(),
                            Some(project_files(dir)),
                        )
                    }
                };
                let mut project = if store == "fresh" {
                    project
                } else {
                    format!("{project}@{store}")
                };
                if net {
                    project.push_str("+net");
                }
                let mut chip = merged.clone();
                if let Some(region) = region {
                    let at = lp_emu_esp32c6::flash::LPFS_OFFSET as usize;
                    chip[at..at + region.len()].copy_from_slice(region);
                }
                // A fresh chip: the first boot formats, the project goes on over
                // the link, and then every measured boot is a power-on.
                let mut builder = Esp32C6Builder::new().boot_mode(BootMode::RomUp);
                if let Some(elf) = p2_elf {
                    builder = builder.app(AppSource::Path(elf.clone()));
                }
                let builder = builder
                    .flash(FlashBacking::Bytes(chip))
                    .reset_cause(ResetCause::UsbUartHpSys)
                    .strap(Strap::App)
                    .time_grade(grade)
                    .usb_host(UsbHost::Attached { draining: true })
                    .usb_sj_queue_source()
                    .reboot_on_reset(true);
                let builder = if net {
                    builder.lan(lan(), ParticipantId(0))
                } else {
                    builder
                };
                let machine = builder.build().expect("the split image builds a machine");
                let mut host = EmuLinkHost::new(
                    C6Board::new(machine).expect("a hosted board"),
                    0x7EE1_0001,
                    true,
                );
                line(&mut host, "\"hello\":{");
                if let Some(files) = &files {
                    let mut client = LpClient::new(&mut host).with_request_ids_from(1_000);
                    block_on(client.replace_and_load_project("first-light", files))
                        .unwrap_or_else(|e| panic!("{label}/{project}: upload: {e}"));
                    // The board makes a load its startup project only once
                    // the project has survived its first frames
                    // (`persist_startup_project`); cut the power before that
                    // and the boot falls back to the lexically first project
                    // — on a store holding another one (`panel-176`'s
                    // `/projects/a`), not this one. Silicon's
                    // `board_first_light` settles for the same reason.
                    line(&mut host, "startup_project set to first-light");
                }
                if net {
                    let mut client = LpClient::new(&mut host).with_request_ids_from(2_000);
                    block_on(client.network_add(
                        NET_SSID.to_string(),
                        lpc_wire::WifiPassword::new(NET_PASSWORD),
                        None,
                    ))
                    .unwrap_or_else(|e| panic!("{label}/{project}: wifi add: {e}"));
                    block_on(client.network_set(Some(true), None))
                        .unwrap_or_else(|e| panic!("{label}/{project}: wifi on: {e}"));
                }
                for boot in 0..BOOTS {
                    host = power_cycle(host, 0x7EE1_0100 + boot as u32);
                    let t0 = host.board.machine.micros();
                    let said = line(&mut host, "[perf] first light ");
                    // The decoder closes a frame on its reset low, after the
                    // firmware has said the line: step on until the lit one is
                    // there (a project), at most a second.
                    let mut lit_us = first_lit_us(&host);
                    let until = host.board.machine.micros() + 1_000_000;
                    while files.is_some() && lit_us.is_none() && host.board.machine.micros() < until
                    {
                        host.step().expect("the run");
                        lit_us = first_lit_us(&host);
                    }
                    let lit_us = lit_us.map(|us| us - t0 as f64);
                    if files.is_some() {
                        assert!(
                            said.contains("first light at "),
                            "{label}/{project}: {said}"
                        );
                        assert!(lit_us.is_some(), "{label}/{project}: no lit frame decoded");
                    } else {
                        assert!(
                            said.contains("first light none by "),
                            "{label}/{project}: {said}"
                        );
                    }
                    // A joining boot: on to its address (`[wifi] address …`).
                    let address = net.then(|| {
                        let l = line(&mut host, "[wifi] address");
                        (l, (host.board.machine.micros() - t0) as f64 / 1e3)
                    });
                    let parts = parts_of(&said);
                    println!(
                        "{label:8} {project:5} boot {boot}: frame on the pad {} | {}",
                        lit_us.map_or("none".to_string(), |us| format!("{:.1} ms", us / 1e3)),
                        said.split("[perf] ").nth(1).unwrap_or(&said)
                    );
                    if let Some((l, ms)) = &address {
                        println!("{label:8} {project:5} boot {boot}: by {ms:.1} ms: {l}");
                    }
                    rows.push(serde_json::json!({
                        "image": label,
                        "project": project,
                        "boot": boot,
                        "line": said,
                        "parts_ms": parts,
                        "pad_first_lit_start_ms": lit_us.map(|us| us / 1e3),
                        "address": address.as_ref().map(|(l, _)| l.clone()),
                        "address_by_ms": address.as_ref().map(|(_, ms)| *ms),
                    }));
                }
            }
        }
    }
    if let Some(dir) = std::env::var("LP_FIRST_LIGHT_DIR")
        .ok()
        .filter(|d| !d.is_empty())
    {
        let out = serde_json::json!({
            "configuration": format!("lp-emu:esp32c6:{grade_name}"),
            "boot": "ROM-up (mask ROM, IDF bootloader, split loader), power-on reset",
            "rows": rows,
        });
        std::fs::create_dir_all(&dir).expect("the records directory");
        std::fs::write(
            std::path::Path::new(&dir).join(format!("emu-first-light-{grade_name}.json")),
            serde_json::to_string_pretty(&out).unwrap(),
        )
        .expect("write the record");
    }
}

/// Step `host` until a console line contains `needle`; that line. Panics
/// with the console's tail if it never comes. With `LP_FIRST_LIGHT_TIMELINE`
/// set, prints every console line with the guest time it arrived by (the
/// end of the slice it came out in).
fn line(host: &mut EmuLinkHost<C6Board>, needle: &str) -> String {
    let timeline = std::env::var_os("LP_FIRST_LIGHT_TIMELINE").is_some();
    let until = host.board.machine.micros() + LINE_BUDGET_US;
    let mut seen = 0;
    loop {
        let now = host.board.machine.micros();
        if timeline {
            for l in &host.console()[seen..] {
                println!("  {:>10.3} ms  {l}", now as f64 / 1e3);
            }
        }
        if let Some(l) = host.console()[seen..].iter().find(|l| l.contains(needle)) {
            return l.clone();
        }
        seen = host.console().len();
        if now >= until {
            panic!("no {needle:?}:\n{}", tail(host));
        }
        if let Err(e) = host.step() {
            panic!("the boot failed before {needle:?} ({e}):\n{}", tail(host));
        }
    }
}

/// The first lit WS281x frame decoded on any pad the guest routed: when its
/// first bit left the chip, guest µs (the `--dump-frames` `start_us`).
fn first_lit_us(host: &EmuLinkHost<C6Board>) -> Option<f64> {
    let m = &host.board.machine;
    m.routed_pads()
        .iter()
        .filter_map(|(pad, _)| {
            m.frames(pad.0)
                .iter()
                .find(|f| f.wire.iter().any(|b| *b != 0))
                .map(|f| f.start as f64 / CYCLES_PER_US as f64)
        })
        .reduce(f64::min)
}

fn tail(host: &EmuLinkHost<C6Board>) -> String {
    let c = host.console();
    c[c.len().saturating_sub(60)..].join("\n")
}

/// Power-cycle the board (every domain to power-on, the flash as it is) and
/// put a fresh host on its link.
fn power_cycle(host: EmuLinkHost<C6Board>, nonce: u32) -> EmuLinkHost<C6Board> {
    let mut board = host.board;
    assert!(board.machine.power_cycle(Strap::App), "a power-on snapshot");
    EmuLinkHost::new(board, nonce, true)
}

/// A LAN of one access point, on the board's own clock.
fn lan() -> SharedLan {
    let mut lan = VirtualLan::new(LanConfig::new(CYCLES_PER_US));
    lan.add_access_point(VirtualAccessPoint {
        name: NET_SSID.to_string(),
        password: Some(NET_PASSWORD.to_string()),
        signal_dbm: -50,
        hidden: false,
    });
    SharedLan::new(lan, LanDriver::SelfDriven)
}

/// The line's `name value` pairs after the colon, as a JSON object.
fn parts_of(line: &str) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    let Some(body) = line.split(" ms: ").nth(1) else {
        return serde_json::Value::Null;
    };
    let words: Vec<&str> = body.split(" (").next().unwrap_or("").split(' ').collect();
    for pair in words.chunks(2) {
        if let [name, value] = pair
            && let Ok(v) = value.parse::<u64>()
        {
            map.insert((*name).to_string(), v.into());
        }
    }
    serde_json::Value::Object(map)
}
