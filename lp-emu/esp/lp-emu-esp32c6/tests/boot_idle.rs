//! G6-2 (as far as M3's flash question allows) and G6-5: the memfs spike
//! image boots through the radio blob to the hello frame on UART0, the
//! boot lines in order, one heartbeat with the §5.4 figures, and the idle
//! loop — strict, no radio `SPIN` left, the `WIFI RX config` line logged.
//!
//! The image is `esp32c6,server,radio,spike_uart0_link,memory_fs` at the
//! reference commit (`scripts/emu/build-reference-image.sh`; `just
//! test-emu-c6` builds it, or set `LP_EMU_C6_REF_BOOT_IDLE_MEMFS`). The
//! flash-backed spike image is the M4 deferral (DD23) and is pinned below
//! as spinning on `SPI1.cmd`.
//!
//! `#[ignore]`d for the usual reason (`test_support`).

use lp_emu_esp_common::trace::SharedBuffer;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, TimeGrade,
};
use lp_emu_esp32c6::test_support::{ReferenceImage, reference_image, skip_notice};

const GATE_US: u64 = 5_500_000;

/// The heap figures the first heartbeat (5 s) reports on this image. The
/// stack line is §5.4's exactly (`11432 B of 71960 B`). The heap is
/// **104 B less free than esp-emu's §5.4 sample** (`freeBytes 266,792`),
/// and P6 established that this is a transient, not a loader fact: the
/// same run's third heartbeat (15 s) reads `266788`, 4 B from esp-emu —
/// the same 4 B the spike report saw between esp-emu and silicon
/// heartbeats on one image (§11.2); the 5 s figure is identical under `t2`
/// (whose stack high-water differs, so interleaving does move samples);
/// and a throwaway "SOF forever" USB model (esp-emu's §4 behaviour)
/// leaves it unchanged. Something of ~100 B lives from boot to past 10 s
/// here and was already gone at esp-emu's 5 s sample — a scheduling
/// difference in the radio blob's dynamic state, which only a silicon
/// capture of this variant can arbitrate (the PR's "G6-2 state"). Pinned
/// as measured for this configuration; never tuned.
const HEARTBEAT_MEMORY: &str =
    r#""memory":{"freeBytes":266688,"usedBytes":58848,"totalBytes":325536"#;
const STACK_LINE: &str = "[stack] heartbeat: high-water 11432 B of 71960 B";

/// The lines the brief asks for, in order.
const BOOT_LINES: &[&str] = &[
    "\nM!{\"id\":0,\"msg\":{\"hello\":{\"proto\":20,",
    "\"boardId\":\"seeed/xiao-esp32-c6\"",
    "\"baseMac\":\"a0:f2:62:87:b4:8c\"",
    "\"chipRevision\":\"0.2\"",
    "Esp32C6RmtWs281xDriver: 2 WS281x channels for 2 declared",
    "[fw-esp32c6] ESP-NOW radio ready: device_id= channel=11",
    "[RECOVERY] boot complete (first frame served)",
    "M!{\"id\":0,\"msg\":{\"heartbeat\":{",
    STACK_LINE,
];

struct Run {
    m: Esp32C6Machine,
    outcome: Outcome,
    text: String,
    notes: Vec<String>,
}

fn machine(image: &ReferenceImage, buf: &SharedBuffer) -> Option<Esp32C6Machine> {
    let elf = match reference_image(image) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("boot_idle", &reason);
            return None;
        }
    };
    Some(
        Esp32C6Builder::new()
            .app(AppSource::Path(elf))
            .strict(true)
            .time_grade(TimeGrade::T1)
            // A filter that matches no block: only the notes (SPIN, TOUCH,
            // WIFI RX config, …) come through.
            .trace(Box::new(buf.clone()), vec!["NOTHING".to_string()])
            .build()
            .expect("the reference image builds a machine"),
    )
}

fn run_memfs() -> Option<Run> {
    let buf = SharedBuffer::new();
    let mut m = machine(&ReferenceImage::BOOT_IDLE_MEMFS, &buf)?;
    let outcome = m.run_until(&StopCondition::after_micros(GATE_US));
    let text = String::from_utf8_lossy(&m.uart0().bytes()).into_owned();
    Some(Run {
        m,
        outcome,
        text,
        notes: buf.lines(),
    })
}

#[test]
#[ignore = "needs the memfs reference image; run through `just test-emu-c6`"]
fn the_memfs_spike_image_says_hello_and_heartbeats_with_the_5_4_figures() {
    let Some(Run {
        m,
        outcome,
        text,
        notes,
    }) = run_memfs()
    else {
        return;
    };
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "expected the emulated deadline, got {outcome:?}"
    );
    assert_eq!(
        m.bus.unmapped_reads() + m.bus.unmapped_writes(),
        0,
        "unmapped"
    );

    // The boot lines, in order.
    let mut from = 0;
    for needle in BOOT_LINES {
        let at = text[from..]
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` not found after byte {from}:\n{text}"));
        from += at + needle.len();
    }
    assert!(text.contains(HEARTBEAT_MEMORY), "{text}");
    // No `[FS]` mount pair: `memory_fs` is the no-flash switch; the flash-
    // backed hello is M4's.
    assert!(!text.contains("[FS]"));

    // The idle loop was reached and stayed: wfi skips through the run.
    assert!(m.idle_skips() > 1_000, "{} idle skips", m.idle_skips());

    // G6-5: no radio SPIN left; the only spin is esp-println's one USB wait.
    let spins: Vec<&String> = notes.iter().filter(|l| l.contains(" SPIN ")).collect();
    assert_eq!(spins.len(), 1, "{spins:?}");
    assert!(spins[0].contains("USB_DEVICE+0x004 ep1_conf"));
    // … and the RX DMA base was logged, from `mac_rxbuf_init`.
    let rx: Vec<&String> = notes
        .iter()
        .filter(|l| l.contains("WIFI RX config: dma_base=0x408"))
        .collect();
    assert_eq!(rx.len(), 1, "{rx:?}");
    // The blob reached well past the calibration: distinct radio offsets.
    let touched = notes
        .iter()
        .filter(|l| l.contains("WIFI_MAC TOUCH"))
        .count();
    assert!(touched > 300, "{touched} distinct WIFI_MAC offsets");
}

#[test]
#[ignore = "needs the memfs reference image; run through `just test-emu-c6`"]
fn two_boot_idle_runs_are_byte_identical() {
    // G6-4, the boot-idle half.
    let (Some(a), Some(b)) = (run_memfs(), run_memfs()) else {
        return;
    };
    assert_eq!(a.outcome, b.outcome);
    assert_eq!(a.m.cycles(), b.m.cycles());
    assert_eq!(a.m.instructions(), b.m.instructions());
    assert_eq!(a.m.idle_skips(), b.m.idle_skips());
    assert_eq!(a.text, b.text, "two runs of the boot-idle image diverged");
    assert_eq!(a.notes, b.notes);
}

#[test]
#[ignore = "needs the flash-backed reference image; run through `just test-emu-c6`"]
fn the_flash_backed_spike_image_spins_on_spi1_cmd_which_is_m4s() {
    // DD23: the shipped boot mounts `lpfs` from flash through esp-storage
    // and the ROM's `esp_rom_spiflash_*`, which is the SPI1 controller.
    let buf = SharedBuffer::new();
    let Some(mut m) = machine(&ReferenceImage::BOOT_IDLE, &buf) else {
        return;
    };
    let outcome = m.run_until(&StopCondition::after_micros(20_000));
    assert!(matches!(outcome, Outcome::Deadline { .. }), "{outcome:?}");
    assert_eq!(m.bus.unmapped_reads() + m.bus.unmapped_writes(), 0);
    assert!(
        buf.lines()
            .iter()
            .any(|l| l.contains("SPIN SPI1+0x000 cmd = 0x10000000")),
        "{:?}",
        buf.lines()
    );
    assert_eq!(m.idle_skips(), 0, "it never reaches the idle loop");
    assert!(m.uart0().is_empty(), "no hello before the mount");
}
