//! **M4 P3's gate:** two emulated C6s running the `espnow-broadcast` payload
//! in lockstep, each printing the other's `event=N` lines — the roadmap's
//! fourth acceptance criterion — and the pair's transcripts, byte-identical
//! across runs.
//!
//! # Why the pair lives in a test and not behind a CLI flag
//!
//! `lp-cli validate record` spawns `lp-emu-esp32c6` **once per capture** and
//! that binary runs **one** machine: there is no `--pair`, and `--air <addr>`
//! (the socket form, RD11's auditable half) does not exist either — P1 shipped
//! the codec, and the listener belongs with plan two's `lp-cli emu serve`.
//! Adding a pair mode to the binary was not this phase's to do: `src/bin/`,
//! `machine.rs` and `lp-cli/.../emu/args.rs` are held by M3 P2, which is in
//! flight.
//!
//! So the pair is driven from here, where [`Lockstep`] already lives, and the
//! two transcripts it writes are **the console bytes of that run**, taken the
//! way the runner takes them: each machine's USB-Serial-JTAG stream, ended at
//! the payload's sentinel — which is exactly what `--exit-on` does on a single
//! machine, and which this payload arranges for by going silent after
//! `=== DONE ===` while still broadcasting (its peer's capture needs it on the
//! air).
//!
//! # Running it
//!
//! `#[ignore]`d and driven by **`LP_EMU_C6_ESPNOW_BROADCAST_ELF`** — a path to
//! an ELF built from `lp-fw/fw-esp32c6` with
//! `--no-default-features --features esp32c6,test_espnow_broadcast`. That image
//! is not one of `test_support`'s named artefacts and every feature set of
//! `fw-esp32c6` builds to the **same** path, so this file takes the path it is
//! given and never builds or guesses one. Without the variable it skips.
//!
//! ```bash
//! cd lp-fw/fw-esp32c6 && cargo build --no-default-features \
//!     --features test_espnow_broadcast,esp32c6 \
//!     --target riscv32imac-unknown-none-elf --profile release-esp32
//! cp target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6 /tmp/espnow-broadcast.elf
//! LP_EMU_C6_ESPNOW_BROADCAST_ELF=/tmp/espnow-broadcast.elf \
//!     cargo test --release -p lp-emu-esp32c6 --test espnow_broadcast_pair -- \
//!     --ignored --nocapture
//! ```
//!
//! Set `LP_EMU_C6_ESPNOW_BROADCAST_OUT=<dir>` as well and the pair writes
//! `machine-a.txt` / `machine-b.txt` there — the console halves of the two
//! committed transcripts.

use lp_emu_esp_common::air::ParticipantId;
use lp_emu_esp32c6::loader::EfuseIdentity;
use lp_emu_esp32c6::lockstep::{DEFAULT_LATENCY_US, Lockstep};
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, StopCondition, TxLogSink, UsbHost,
};
use lp_emu_esp32c6::memmap;

/// The desk board — `d1-desk-batch.md`'s "the desk board", every existing
/// reference transcript's chip. Machine A carries its MAC so that `device=`
/// compares against a silicon capture rather than needing a mask.
const MAC_A: &str = "a0:f2:62:87:b4:8c";
/// The second board, M4's.
const MAC_B: &str = "a0:f2:62:85:a8:7c";

/// The device ids those MACs give, through the product driver's own
/// `station_device_id` (the low four bytes of the station MAC, little-endian).
const DEVICE_A: u32 = 0x8cb4_8762;
const DEVICE_B: u32 = 0x7ca8_8562;

/// Machine B's power-on offset in the pair's clock.
///
/// Two identical images started on the same cycle do the same thing on the
/// same cycle, and a stagger is what two real boards have — nobody powers two
/// of them up together. It is also the shape the desk capture has, and by a
/// much larger margin: there, the board that is not being recorded has been up
/// since the previous flash. A quarter of a second is enough for the two
/// machines' sends to land at different points in each other's tick loop,
/// which is the interesting case; determinism is untouched, because an offset
/// is a constant in guest cycles (`Lockstep::stagger`).
const STAGGER_MS: u64 = 250;

/// The stagger, overridable with `LP_EMU_C6_ESPNOW_BROADCAST_STAGGER_MS`.
///
/// The knob exists because the first pair run of this payload found the
/// receiving guest reporting **every other** frame, and "is that a phase
/// artefact of two machines a fixed distance apart, or is it structural?" is a
/// question one environment variable answers in a second. It is not a knob a
/// committed transcript ever uses: the recorded pair is [`STAGGER_MS`].
fn stagger_ms() -> u64 {
    std::env::var("LP_EMU_C6_ESPNOW_BROADCAST_STAGGER_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(STAGGER_MS)
}

/// How far the pair is run. The payload is ready at about 1.0 s, sends every
/// 100 ms and needs six sends and six received frames; with B a quarter second
/// behind, both sentinels are comfortably inside this.
const HORIZON_MS: u64 = 3_500;

fn elf() -> Option<String> {
    match std::env::var("LP_EMU_C6_ESPNOW_BROADCAST_ELF") {
        Ok(path) if std::path::Path::new(&path).is_file() => Some(path),
        Ok(path) => panic!("LP_EMU_C6_ESPNOW_BROADCAST_ELF={path} is not a file"),
        Err(_) => {
            eprintln!(
                "espnow_broadcast_pair: skipped — set LP_EMU_C6_ESPNOW_BROADCAST_ELF to a \
                 `test_espnow_broadcast,esp32c6` ELF (see this file's docs)"
            );
            None
        }
    }
}

fn ms(n: u64) -> u64 {
    n * 1_000 * memmap::CYCLES_PER_US
}

fn machine(elf: &str, mac: &str, tx_log: TxLogSink) -> Esp32C6Machine {
    Esp32C6Builder::new()
        .app(AppSource::Path(elf.into()))
        .tx_log(tx_log)
        .usb_host(UsbHost::Attached { draining: true })
        .efuse(EfuseIdentity {
            mac: EfuseIdentity::parse_mac(mac).expect("a MAC"),
            ..EfuseIdentity::default()
        })
        .build()
        .expect("the espnow-broadcast image builds a machine")
}

/// One run of the pair. Returns each machine's console, ended at the payload's
/// sentinel the way a host-side `--exit-on` ends a capture.
fn run_the_pair(elf: &str, tx_log: TxLogSink) -> (String, String) {
    let a = machine(elf, MAC_A, tx_log);
    let b = machine(elf, MAC_B, TxLogSink::Off);
    let mut pair = Lockstep::new(vec![a, b])
        .expect("a pair")
        .stagger(vec![0, ms(stagger_ms())]);
    let report = pair.run_until(ms(HORIZON_MS), &StopCondition::default());
    for m in &report.machines {
        let machine = pair.machine(m.id).expect("a machine");
        eprintln!(
            "machine {}: cycles={} sent={} offered={} delivered={} undelivered={} outcome={:?}",
            m.id,
            m.cycles,
            m.frames_sent,
            m.frames_offered,
            machine.air_frames_delivered(),
            machine.air_frames_undelivered(),
            m.outcome
        );
    }
    let console = |id: ParticipantId| -> String {
        let text = pair.machine(id).expect("a machine").usb_sj().text();
        capture_window(&text)
    };
    (console(ParticipantId(0)), console(ParticipantId(1)))
}

/// The bytes a host on the port would have kept: everything up to and
/// including the sentinel.
///
/// This is not an edit of a transcript — it is the capture window, and it is
/// the same window `--exit-on` gives a single machine. The payload goes silent
/// after the sentinel (while still broadcasting, so its peer's capture works),
/// so in practice there is nothing after it to drop.
fn capture_window(text: &str) -> String {
    const DONE: &str = "[espnow-broadcast] === DONE ===";
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        out.push_str(line);
        if line.contains(DONE) {
            break;
        }
    }
    out
}

/// Every `event=N` this console reports **hearing**, in order.
fn heard(console: &str) -> Vec<(u32, u32)> {
    console
        .lines()
        .filter_map(|line| {
            let rest = line.split("[espnow-broadcast] rx device=0x").nth(1)?;
            let (device, rest) = rest.split_once(' ')?;
            let event = rest.strip_prefix("event=")?.split(' ').next()?;
            Some((
                u32::from_str_radix(device, 16).ok()?,
                event.parse::<u32>().ok()?,
            ))
        })
        .collect()
}

/// **G3-1. The pair hears itself, on the payload.**
///
/// Two emulated C6s running `espnow-broadcast` in lockstep, each carrying one
/// of the two desk boards' MACs, each printing the other's `event=N` lines for
/// N ≥ 2 — the plan's acceptance criterion 4, in full.
#[test]
#[ignore = "needs an espnow-broadcast ELF in LP_EMU_C6_ESPNOW_BROADCAST_ELF"]
fn the_pair_hears_the_other_boards_events() {
    let Some(elf) = elf() else { return };
    let tx_log = std::env::temp_dir().join("c6r-m4-p3-pair-tx.log");
    let _ = std::fs::remove_file(&tx_log);
    let (console_a, console_b) = run_the_pair(&elf, TxLogSink::File(tx_log.clone()));
    println!("--- machine A ({MAC_A}) ---\n{console_a}");
    println!("--- machine B ({MAC_B}) ---\n{console_b}");

    for (label, console, own, peer) in [
        ("A", &console_a, DEVICE_A, DEVICE_B),
        ("B", &console_b, DEVICE_B, DEVICE_A),
    ] {
        assert!(
            console.contains(&format!("radio ready device=0x{own:08x}")),
            "machine {label}'s radio never came up:\n{console}"
        );
        assert!(
            console.contains("[espnow-broadcast] === DONE ==="),
            "machine {label} never reached the sentinel:\n{console}"
        );
        let heard = heard(console);
        assert!(
            !heard.is_empty(),
            "machine {label} heard nothing at all:\n{console}"
        );
        assert!(
            heard.iter().all(|(device, _)| *device == peer),
            "machine {label} heard a device that is not its peer: {heard:?}"
        );
        // The criterion is `event=N` for N >= 2, from the OTHER machine.
        assert!(
            heard.iter().any(|(_, event)| *event >= 2),
            "machine {label} never heard its peer's event 2 or later: {heard:?}"
        );
        // Six of each, which is the shape a replay compares.
        assert_eq!(
            console.matches(r#"{"kind":"espnow-tx""#).count(),
            6,
            "machine {label} did not record six tx events:\n{console}"
        );
        assert_eq!(
            console.matches(r#"{"kind":"espnow-rx""#).count(),
            6,
            "machine {label} did not record six rx events:\n{console}"
        );
        // Every frame that arrives arrives whole: the byte count is the one
        // the peer's own event number prescribes, on every record.
        assert_eq!(
            console.matches(r#""len_ok":false"#).count(),
            0,
            "machine {label} received a frame of the wrong length:\n{console}"
        );
    }

    if let Ok(dir) = std::env::var("LP_EMU_C6_ESPNOW_BROADCAST_OUT") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("the output directory");
        std::fs::write(dir.join("machine-a.txt"), &console_a).expect("machine A's console");
        std::fs::write(dir.join("machine-b.txt"), &console_b).expect("machine B's console");
        eprintln!(
            "wrote {} and {}; the air's stated latency is {DEFAULT_LATENCY_US} us",
            dir.join("machine-a.txt").display(),
            dir.join("machine-b.txt").display()
        );
    }
}

/// **The finding this phase did not go looking for, pinned.**
///
/// A guest on this emulator's air reports **every other** frame written into
/// its RX ring. The air is not losing them and the ring is not refusing them —
/// `frames_offered`, `air_frames_delivered` and the sender's `frames_sent` all
/// agree, and `air_frames_undelivered` is 0 — but the receiving application
/// sees the peer's events 0, 2, 4, 6, 8, 10 and never an odd one.
///
/// It is **not** a phase artefact of two machines a fixed distance apart:
/// `LP_EMU_C6_ESPNOW_BROADCAST_STAGGER_MS` was swept and 25 ms (where the
/// peer's frames land nowhere near this machine's own sends) gives exactly the
/// same alternation as 250 ms. It is not the guest's own transmit interfering
/// either, for the same reason.
///
/// Nothing before this payload could have found it. M4 P2 delivered **one**
/// frame into a ring and M4 U1 completed **one** transmission; this is the
/// first time anything on this machine has sent and received repeatedly, which
/// is the whole point of a two-board payload.
///
/// The mechanism is `lp-emu-esp32c6/src/periph/wifi_stub.rs`'s — M4 P2's,
/// merged, and fenced for this phase — so this phase **reports and pins** it
/// rather than reaching into it (M4 P3 scope 5: "never tune a number toward
/// silicon"). `docs/debt/emu-c6-air-delivers-every-other-frame.md` carries the
/// question. This test fails the day it is fixed, which is the point of
/// pinning it.
#[test]
#[ignore = "needs an espnow-broadcast ELF in LP_EMU_C6_ESPNOW_BROADCAST_ELF"]
fn the_air_surfaces_only_every_other_delivered_frame() {
    let Some(elf) = elf() else { return };
    let (console_a, console_b) = run_the_pair(&elf, TxLogSink::Off);
    for (label, console) in [("A", &console_a), ("B", &console_b)] {
        assert_eq!(
            console.matches(r#""gap":2"#).count(),
            5,
            "machine {label} no longer alternates — if the air was fixed, this test is the \
             record of what it used to do and the payload's transcripts want re-recording:\n\
             {console}"
        );
        assert_eq!(
            console.matches(r#""gap":1"#).count(),
            0,
            "machine {label} saw a contiguous pair, which the alternation says is \
             impossible:\n{console}"
        );
    }
}

/// **G3-2. Determinism.** Two runs of the pair are byte-identical, on both
/// machines.
///
/// It is not a property of the payload — it is RD11's whole reason for putting
/// two machines in one thread rather than two: the interleaving is fixed by the
/// quantum, so nothing about a delivery depends on how fast the host is.
#[test]
#[ignore = "needs an espnow-broadcast ELF in LP_EMU_C6_ESPNOW_BROADCAST_ELF"]
fn two_runs_of_the_pair_are_byte_identical() {
    let Some(elf) = elf() else { return };
    let first = run_the_pair(&elf, TxLogSink::Off);
    let second = run_the_pair(&elf, TxLogSink::Off);
    assert_eq!(first.0, second.0, "machine A's console moved between runs");
    assert_eq!(first.1, second.1, "machine B's console moved between runs");
    assert!(!first.0.is_empty() && !first.1.is_empty());
}
