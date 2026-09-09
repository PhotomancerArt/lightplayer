//! M4 P2's gates: a frame delivered into a running guest's RX ring, and
//! **what the guest itself did with each thing we put there**.
//!
//! Most of this file is not a pass/fail gate. It is the experiment that
//! decided the design, kept so that the next person can re-run it rather
//! than trust a comment: which interrupt bit reaches the RX path, which
//! registers the blob reads on the way, how far up its own call chain a
//! delivery gets, and which of the 92 bytes of the `rx_ctrl` header the
//! guest's behaviour actually depends on. That last one is the phase's
//! second deliverable after the `rx` line itself, and it is
//! [`which_header_bytes_the_guest_notices`].
//!
//! # Running it
//!
//! `#[ignore]`d and driven by **`LP_EMU_C6_ESPNOW_ELF`** — a path to an ELF
//! built from `lp-fw/fw-esp32c6` with
//! `--no-default-features --features test_espnow,esp32c6`. That image is not
//! one of `test_support`'s named artefacts, and every feature set of
//! `fw-esp32c6` builds to the **same** path, so this file takes the path it
//! is given and never builds or guesses one. Without the variable it skips.
//!
//! ```bash
//! cd lp-fw/fw-esp32c6 && cargo build --no-default-features \
//!     --features test_espnow,esp32c6 \
//!     --target riscv32imac-unknown-none-elf --profile release-esp32
//! cp target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6 /tmp/espnow.elf
//! LP_EMU_C6_ESPNOW_ELF=/tmp/espnow.elf \
//!     cargo test --release -p lp-emu-esp32c6 --test air_delivery -- --ignored --nocapture
//! ```

use lp_emu_esp_common::air::{AirFrame, ParticipantId};
use lp_emu_esp_common::trace::SharedBuffer;
use lp_emu_esp32c6::loader::EfuseIdentity;
use lp_emu_esp32c6::lockstep::Lockstep;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, TxLogSink, UsbHost,
};
use lp_emu_esp32c6::memmap;

/// The 56-byte ESP-NOW broadcast frame the `test_espnow` image arms, read
/// out of guest RAM by `--tx-log` on a machine whose eFuse MAC is the
/// bench's second board (`a0:f2:62:85:a8:7c`). Byte for byte what the
/// sender armed — `addr1` and `addr3` broadcast, `addr2` that MAC, category
/// `0x7f`, Espressif OUI, ESP-NOW element type 4.
const FRAME_FROM_THE_OTHER_BOARD: &str = "d0000000ffffffffffffa0f26285a87cffffffffffff00007f18fe34\
e2b3830ddd1618fe340402504c01016285a87c000000000100000000";

/// The `WIFI_MAC` window's base, for the experiments that read the RX
/// registers back.
const WIFI_MAC: u32 = 0x600a_0000;

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("hex"))
        .collect()
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn espnow_elf() -> Option<String> {
    match std::env::var("LP_EMU_C6_ESPNOW_ELF") {
        Ok(path) if std::path::Path::new(&path).is_file() => Some(path),
        Ok(path) => panic!("LP_EMU_C6_ESPNOW_ELF={path} is not a file"),
        Err(_) => {
            eprintln!(
                "air_delivery: skipped — set LP_EMU_C6_ESPNOW_ELF to a \
                 `test_espnow,esp32c6` ELF (see this file's docs)"
            );
            None
        }
    }
}

fn machine(elf: &str, mac: &str) -> Esp32C6Machine {
    build(elf, mac, None, TxLogSink::Off)
}

fn traced_machine(elf: &str, mac: &str, buf: &SharedBuffer) -> Esp32C6Machine {
    build(elf, mac, Some(buf), TxLogSink::Off)
}

fn build(elf: &str, mac: &str, buf: Option<&SharedBuffer>, tx_log: TxLogSink) -> Esp32C6Machine {
    let mut b = Esp32C6Builder::new()
        .app(AppSource::Path(elf.into()))
        .tx_log(tx_log);
    if let Some(buf) = buf {
        b = b.trace(
            Box::new(buf.clone()),
            vec!["WIFI_MAC".to_string(), "WIFI_PWR".to_string()],
        );
    }
    b.usb_host(UsbHost::Attached { draining: true })
        .efuse(EfuseIdentity {
            mac: EfuseIdentity::parse_mac(mac).expect("a MAC"),
            ..EfuseIdentity::default()
        })
        .build()
        .expect("the test_espnow image builds a machine")
}

fn ms(n: u64) -> u64 {
    n * 1_000 * memmap::CYCLES_PER_US
}

fn until(cycle: u64) -> StopCondition {
    StopCondition {
        stop_cycle: Some(cycle),
        ..StopCondition::default()
    }
}

/// A machine run to 500 ms — radio up, four ticks before its own first send
/// — with a seat on an air, ready to be offered frames.
fn a_listening_machine(elf: &str) -> Esp32C6Machine {
    let mut m = machine(elf, "a0:f2:62:87:b4:8c");
    m.arm_air(ParticipantId(0));
    m.run_until(&until(ms(500)));
    assert!(
        m.usb_sj().text().contains("[test_espnow] radio ready"),
        "the radio never came up"
    );
    m
}

fn offer(m: &mut Esp32C6Machine, at: u64, bytes: Vec<u8>) {
    m.offer_air_frame(&AirFrame {
        from: ParticipantId(1),
        at,
        due: at,
        bytes,
    });
}

/// **G2-1 and G2-2.** Two machines on one air, and the receiving guest's own
/// application prints the frame the sending guest's blob armed.
///
/// # What this gate is — amended by M4 U1
///
/// P2 wrote this gate as a one-way claim, because at the time the
/// `test_espnow` guest armed exactly one frame and never returned from its
/// own `send`: one frame per machine ever, and the machine that sent
/// *second* was always sending into a machine that had already stopped
/// draining.
///
/// **M4 U1 closed that debt** — a machine on an air completes its
/// transmissions — so this run is now a **conversation**: both guests print
/// the other's frame *and* their own `tx` line. The gate is amended to say
/// so rather than left asserting a wedge that no longer exists.
///
/// The stagger stays. It is no longer load-bearing for the printing (see
/// `the_symmetric_pair_talks_both_ways` below, which was P2's
/// `the_symmetric_pair_delivers_but_cannot_print`), but it is what makes the
/// two machines' sends land at different times, which is the interesting
/// case.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn the_pair_hears_itself() {
    let Some(elf) = espnow_elf() else { return };
    let log = std::env::temp_dir().join("c6r-m4-p2-pair-tx.log");
    let _ = std::fs::remove_file(&log);
    let a = build(
        &elf,
        "a0:f2:62:87:b4:8c",
        None,
        TxLogSink::File(log.clone()),
    );
    let b = machine(&elf, "a0:f2:62:85:a8:7c");
    let mut pair = Lockstep::new(vec![a, b])
        .expect("a pair")
        .stagger(vec![0, ms(250)]);
    let report = pair.run_until(ms(1_400), &StopCondition::default());

    let (first, second) = (ParticipantId(0), ParticipantId(1));
    let console_a = pair.machine(first).expect("a").usb_sj().text();
    let console_b = pair.machine(second).expect("b").usb_sj().text();
    println!("--- machine 0 (the sender) ---\n{console_a}");
    println!("--- machine 1 (250 ms behind) ---\n{console_b}");
    for m in &report.machines {
        println!(
            "machine {}: sent={} offered={} outcome={:?}",
            m.id, m.frames_sent, m.frames_offered, m.outcome
        );
    }

    // Both armed their one frame in this window; the air carried both; and
    // now that a TX completes, **both** guests drained the other's and both
    // printed their own `tx` line.
    assert_eq!(report.machines[0].frames_sent, 1);
    assert_eq!(report.machines[1].frames_sent, 1);
    assert_eq!(pair.machine(second).expect("b").air_frames_delivered(), 1);
    for (label, console) in [("machine 0", &console_a), ("machine 1", &console_b)] {
        assert!(
            console.contains("[test_espnow] rx "),
            "{label} never printed an rx line:\n{console}"
        );
        assert!(
            console.contains("[test_espnow] tx simulated_button device= event=1"),
            "{label}'s own send never returned:\n{console}"
        );
    }

    // G2-2, the hex triple: the bytes the sender's own `--tx-log` printed,
    // the bytes the air carried, and the bytes read back out of the
    // receiver's RX buffer. The middle one is the same `Vec` by
    // construction (`take_air_frames` hands the machine's reading straight
    // to the air), so the two ends are what has to be shown equal.
    let m = pair.machine_mut(second).expect("b");
    let base = m.peek_word(WIFI_MAC + 0x4084).expect("the ring base");
    let buf = m.peek_word(base + 4).expect("the first buffer");
    let header = lp_emu_esp32c6::periph::wifi_stub::rx_ctrl::LEN as u32;
    let received: Vec<u8> = (0..56u32)
        .map(|i| {
            let at = buf + header + i;
            let word = m.peek_word(at & !3).expect("the buffer reads");
            word.to_le_bytes()[(at & 3) as usize]
        })
        .collect();
    // The sender's log is behind a buffered writer the machine owns, so the
    // machines have to go before the file can be read.
    drop(pair.into_machines());
    let line = std::fs::read_to_string(&log).expect("the sender's tx log");
    println!("{}", line.trim());
    let sent = line
        .split("frame=")
        .nth(1)
        .expect("a frame= field")
        .trim()
        .to_string();
    println!("tx-log frame   : {sent}");
    println!("in the RX ring : {}", bytes_to_hex(&received));
    assert_eq!(
        sent,
        bytes_to_hex(&received),
        "the bytes in the receiver's buffer are not the bytes the sender armed"
    );
}

/// P2 wrote this as `the_symmetric_pair_delivers_but_cannot_print`, and said
/// in its own docstring: "if this ever starts printing, a TX has completed
/// and `docs/debt/emu-c6-radio-tx-never-completes.md` is closed — which would
/// be very good news and should be chased, not silenced."
///
/// **M4 U1 chased it.** Two identical images started in the same cycle both
/// deliver *and* both print — the frame they received and their own `tx`
/// line — because neither is wedged in its own `send` any more. The gate is
/// the same run with the opposite expectation, kept under a new name so the
/// change is visible in a diff rather than hidden in an assertion.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn the_symmetric_pair_talks_both_ways() {
    let Some(elf) = espnow_elf() else { return };
    let a = machine(&elf, "a0:f2:62:87:b4:8c");
    let b = machine(&elf, "a0:f2:62:85:a8:7c");
    let mut pair = Lockstep::new(vec![a, b]).expect("a pair");
    let report = pair.run_until(ms(1_200), &StopCondition::default());
    for i in 0..2 {
        let m = pair.machine(ParticipantId(i)).expect("a machine");
        let console = m.usb_sj().text();
        println!("--- machine {i} ---\n{console}");
        assert_eq!(report.machines[i].frames_sent, 1);
        assert_eq!(m.air_frames_delivered(), 1, "the ring took it");
        assert!(
            console.contains("[test_espnow] rx "),
            "machine {i} never printed an rx line:\n{console}"
        );
        assert!(
            console.contains("[test_espnow] tx simulated_button device= event=1"),
            "machine {i}'s own send never returned:\n{console}"
        );
    }
}

/// **G2-5, the ring's end.** The blob posts ten descriptors and the chain
/// ends in a NULL rather than wrapping. Twelve frames into a guest that
/// cannot drain them fills it, and the eleventh and twelfth are **dropped,
/// counted and logged once** — never silently.
///
/// The observed behaviour that decided the policy is in the run: this guest
/// does **not** re-post a descriptor the air filled (`owner` stays clear on
/// every one of the ten afterwards), so following the chain from the base on
/// every delivery is not enough on its own and the drop-count-log rule
/// stands.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn the_eleventh_frame_is_dropped_counted_and_logged() {
    let Some(elf) = espnow_elf() else { return };
    let mut m = a_listening_machine(&elf);
    let bytes = hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD);
    for n in 0..12u64 {
        offer(&mut m, ms(500) + n, bytes.clone());
        // No running in between: the point is a ring nobody has emptied.
    }
    println!(
        "offered={} delivered={} undelivered={}",
        m.air_frames_offered(),
        m.air_frames_delivered(),
        m.air_frames_undelivered()
    );
    assert_eq!(m.air_frames_offered(), 12);
    assert_eq!(m.air_frames_delivered(), 10, "ten descriptors, ten frames");
    assert_eq!(m.air_frames_undelivered(), 2);

    // And the ring really is the guest's ten, all handed back.
    let base = m.peek_word(WIFI_MAC + 0x4084).expect("the ring base");
    let mut desc = base;
    let mut owned_by_hardware = 0;
    let mut n = 0;
    while n < 16 {
        let dw0 = m.peek_word(desc).expect("a descriptor");
        if dw0 & (1 << 31) != 0 {
            owned_by_hardware += 1;
        }
        n += 1;
        match m.peek_word(desc + 8).expect("a link") {
            0 => break,
            next => desc = next,
        }
    }
    println!("ring: {n} descriptors, {owned_by_hardware} still the hardware's");
    assert_eq!(n, 10, "the ring the blob posted");
    assert_eq!(owned_by_hardware, 0, "all ten were filled and handed back");
}

/// **G2-7, determinism.** Two runs of the same staggered pair are the same
/// run: same cycles, same frames, same consoles.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn two_runs_of_the_pair_are_identical() {
    let Some(elf) = espnow_elf() else { return };
    let run = || {
        let a = machine(&elf, "a0:f2:62:87:b4:8c");
        let b = machine(&elf, "a0:f2:62:85:a8:7c");
        let mut pair = Lockstep::new(vec![a, b])
            .expect("a pair")
            .stagger(vec![0, ms(250)]);
        let report = pair.run_until(ms(1_400), &StopCondition::default());
        let consoles: Vec<String> = (0..2)
            .map(|i| pair.machine(ParticipantId(i)).expect("m").usb_sj().text())
            .collect();
        let instructions: Vec<u64> = (0..2)
            .map(|i| pair.machine(ParticipantId(i)).expect("m").instructions())
            .collect();
        (report, consoles, instructions)
    };
    let first = run();
    let second = run();
    assert_eq!(first.0, second.0);
    assert_eq!(first.1, second.1);
    assert_eq!(first.2, second.2, "instruction counts");
    println!(
        "two runs, identical: instructions {:?}, rx on machine 1 = {}",
        first.2,
        first.1[1].contains("[test_espnow] rx ")
    );
}

/// **G2-7, the off switch.** A machine that is not in an air is the machine
/// that came before this phase: the same instruction count, to the
/// instruction, as a plain run of the same image.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn a_machine_not_in_an_air_retires_the_same_instructions() {
    let Some(elf) = espnow_elf() else { return };
    let mut plain = machine(&elf, "a0:f2:62:87:b4:8c");
    plain.run_until(&until(ms(1_500)));
    println!(
        "no air: {} instructions at 1,500 ms, {} bytes of console",
        plain.instructions(),
        plain.usb_sj().text().len()
    );
    // The figure M4 P0 §7 measured with the TX log off, with it on, and
    // before the flag existed. It must not have moved.
    assert_eq!(plain.instructions(), 79_871_852);
}

/// Every bit of the MAC's event word, against the RX oracle, **with a frame
/// already in the ring** — the sweep M4 P1 ran with an empty one and read as
/// "the value never reaches a dispatch".
///
/// It does. Eight distinct instruction counts across 35 candidates, and bit
/// 14 is the only one that reaches the RX registers at all.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn every_event_bit_against_the_rx_oracle() {
    let Some(elf) = espnow_elf() else { return };
    let mut rows = Vec::new();
    for bits in (0..32u32)
        .map(|n| 1u32 << n)
        .chain([0xffff_ffff, 0x19a8_79e0])
    {
        let buf = SharedBuffer::new();
        let mut m = traced_machine(&elf, "a0:f2:62:87:b4:8c", &buf);
        m.arm_air(ParticipantId(0));
        let i = m
            .bus
            .peripheral_index("WIFI_MAC")
            .expect("the radio window");
        m.bus
            .with_peripheral::<lp_emu_esp32c6::periph::wifi_stub::WifiStub, _>(i, |w, _| {
                w.set_rx_event_bits(bits)
            });
        m.run_until(&until(ms(500)));
        let mark = buf.lines().len();
        offer(
            &mut m,
            ms(500) - 1,
            hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD),
        );
        m.run_until(&until(ms(700)));
        let lines = buf.lines();
        let after: Vec<&String> = lines.iter().skip(mark).collect();
        rows.push((
            bits,
            m.instructions(),
            m.usb_sj().text().contains("[test_espnow] rx "),
            // The RX cursor and the last-filled register: nothing but the RX
            // path reads them.
            after
                .iter()
                .any(|l| l.contains("+0x4088") || l.contains("+0x408c")),
        ));
    }
    println!("bits        instructions  rx-line  reached-the-rx-registers");
    for (bits, ins, rx, rx_regs) in &rows {
        println!("{bits:#010x}  {ins:>12}  {rx:>7}  {rx_regs:>24}");
    }
    let reached: Vec<u32> = rows
        .iter()
        .filter(|r| r.3)
        .map(|r| r.0)
        .filter(|b| b.count_ones() == 1)
        .collect();
    assert_eq!(
        reached,
        vec![1 << 14],
        "bit 14 is the only single bit that reaches the RX path"
    );
    let distinct: std::collections::BTreeSet<u64> = rows.iter().map(|r| r.1).collect();
    println!(
        "distinct instruction counts across {} candidates: {}",
        rows.len(),
        distinct.len()
    );
    assert!(
        distinct.len() > 1,
        "M4 P1 saw one; with a frame in the ring there are several"
    );
}

/// **The absurd-value test the brief asked for, byte by byte.** Every one of
/// the header's 92 bytes set to `0xff` in turn; a byte whose value the guest
/// never notices is a byte we invented.
///
/// The answer, and it is the phase's second deliverable: **eight bytes of
/// 92**. Byte 1 (`rate`), byte 3 (`rxmatch0`, and only its bit 4), bytes 33
/// and 34 (`rx_channel_estimate_len` and its valid bit), byte 39
/// (`cur_bb_format` / `cur_single_mpdu`), bytes 84 and 85 (`sig_len`) and
/// byte 88 (`rx_state`).
///
/// Everything else — `rssi`, `noise_floor`, `channel`, `second`,
/// `timestamp`, `he_siga1`, `he_siga2`, `rxend_state`, `is_group`,
/// `dump_len` — the guest never looks at on this path. Those are fields
/// **we invented**, and the README says so in those words.
///
/// The list is longer than the one an earlier round of this experiment
/// found, and that is worth knowing: before `rxmatch0` was set the frame was
/// rejected early and only three bytes could matter. **A field-sensitivity
/// list measured on a rejected frame is worthless**; this one is measured on
/// a frame that reaches the application.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn which_header_bytes_the_guest_notices() {
    let Some(elf) = espnow_elf() else { return };
    let run = |poke: Option<(u32, u8)>| -> u64 {
        let mut m = a_listening_machine(&elf);
        offer(
            &mut m,
            ms(500) - 1,
            hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD),
        );
        if let Some((byte, value)) = poke {
            let base = m.peek_word(WIFI_MAC + 0x4084).expect("the ring base");
            let buf = m.peek_word(base + 4).expect("the buffer");
            let word_at = buf + (byte & !3);
            let mut b = m.peek_word(word_at).expect("a word").to_le_bytes();
            b[(byte & 3) as usize] = value;
            m.poke_word(word_at, u32::from_le_bytes(b));
        }
        m.run_until(&until(ms(800)));
        m.instructions()
    };
    let baseline = run(None);
    let mut noticed = Vec::new();
    for byte in 0..lp_emu_esp32c6::periph::wifi_stub::rx_ctrl::LEN as u32 {
        if run(Some((byte, 0xff))) != baseline {
            noticed.push(byte);
        }
    }
    println!("baseline instructions={baseline}");
    println!("header bytes whose value the guest noticed: {noticed:?}");
    assert_eq!(
        noticed,
        vec![1, 3, 33, 34, 39, 84, 85, 88],
        "rate, rxmatch, the channel-estimate pair, cur_bb_format, sig_len \
         and rx_state — and nothing else in 92 bytes"
    );
}

/// Which value of the one byte that gates the frame: every value of header
/// byte 3, against `--break-at ppRxPkt`.
///
/// 128 values reach it and 128 do not, and the split is exactly bit 4 —
/// `rxmatch0`.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn only_rxmatch0_gets_the_frame_into_pprxpkt() {
    let Some(elf) = espnow_elf() else { return };
    let mut hits = Vec::new();
    for value in 0..=255u32 {
        let mut m = a_listening_machine(&elf);
        let _ = m.break_at("ppRxPkt");
        offer(
            &mut m,
            ms(500) - 1,
            hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD),
        );
        let base = m.peek_word(WIFI_MAC + 0x4084).expect("the ring base");
        let buf = m.peek_word(base + 4).expect("the buffer");
        let mut b = m.peek_word(buf).expect("a word").to_le_bytes();
        b[3] = value as u8;
        m.poke_word(buf, u32::from_le_bytes(b));
        if matches!(m.run_until(&until(ms(700))), Outcome::Breakpoint { .. }) {
            hits.push(value);
        }
    }
    println!(
        "values of header byte 3 that reach ppRxPkt: {} of 256",
        hits.len()
    );
    assert_eq!(hits.len(), 128);
    assert!(
        hits.iter().all(|v| v & 0x10 != 0),
        "and every one of them has bit 4 set: {hits:?}"
    );
}

/// How far up the blob's own RX chain a delivery gets: one breakpoint per
/// candidate symbol, each in its own run. The symbols come from the image's
/// own symbol table; no blob code was read.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn how_far_up_the_rx_chain_a_delivery_gets() {
    let Some(elf) = espnow_elf() else { return };
    let mut entered = Vec::new();
    for sym in [
        "lmacProcessRxSucData",
        "wdevProcessRxSucDataAll",
        "wDev_ProcessRxSucData",
        "hal_mac_rx_get_last_dscr",
        "hal_mac_rx_read_rxdscrnext",
        "ppRxPkt",
        "ppProcessRxPktHdr",
        "ppEnqueueRxq",
        "wdev_record_rx_linked_list",
    ] {
        let mut m = a_listening_machine(&elf);
        let resolved = m.break_at(sym).ok();
        offer(
            &mut m,
            ms(500) - 1,
            hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD),
        );
        let outcome = m.run_until(&until(ms(800)));
        let hit = matches!(outcome, Outcome::Breakpoint { .. });
        println!(
            "{sym:28} {:>12} {}",
            resolved.map(|a| format!("{a:#010x}")).unwrap_or_default(),
            if hit { "ENTERED" } else { "not entered" }
        );
        if hit {
            entered.push(sym);
        }
    }
    for must in ["wDev_ProcessRxSucData", "ppRxPkt", "ppEnqueueRxq"] {
        assert!(
            entered.contains(&must),
            "a delivered frame must reach {must}: {entered:?}"
        );
    }
}

/// The descriptor word's undetermined bits, tested rather than argued: the
/// guest's behaviour with the byte count in `[23:12]`, with the 2,704 the
/// ring already held, with `[28:24]` cleared, and with `owner` left set.
///
/// Three results, and two of them were not expected:
///
/// - **`[23:12]` is tolerated at every value tried** — the byte count, the
///   2,704 the ring already held, all ones — and the frame arrives in each.
///   But the instruction counts are *not* identical, so the guest **reads**
///   the field even though it accepts anything in it. "Undetermined" is the
///   honest word, not "ignored", and the byte count is written because that
///   is what a reader would expect to find there.
/// - **`[28:24]` is load-bearing.** M4 P0 saw 0 on the TX descriptor and 1
///   on a posted RX one and could not explain it; clearing it here stops
///   the guest receiving outright. Its *meaning* is still M4 P0's U3, but
///   "leave it exactly as the blob posted it" is now a rule with a test
///   behind it rather than a caution.
/// - **`owner` left set changes nothing.** Clearing it is the `lldesc`
///   convention and this machine does it, but the guest does not check it —
///   so that bit is our convention, not its requirement.
///
/// `eof` (`[30]`) is the one bit of the word this phase sets that the guest
/// does demand.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn the_descriptor_words_undetermined_bits_change_nothing() {
    let Some(elf) = espnow_elf() else { return };
    let run = |dw0: Option<u32>| -> (u64, bool) {
        let mut m = a_listening_machine(&elf);
        offer(
            &mut m,
            ms(500) - 1,
            hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD),
        );
        let base = m.peek_word(WIFI_MAC + 0x4084).expect("the ring base");
        if let Some(dw0) = dw0 {
            m.poke_word(base, dw0);
        }
        m.run_until(&until(ms(800)));
        (
            m.instructions(),
            m.usb_sj().text().contains("[test_espnow] rx "),
        )
    };
    let delivered = run(None);
    println!("as delivered                       -> {delivered:?}");
    for (label, dw0, expect_rx) in [
        // Tolerated: every value of `[23:12]` still delivers.
        ("[23:12] left at the ring's 2,704", 0x41a9_06a4u32, true),
        ("[23:12] all ones", 0x41ff_f6a4, true),
        ("owner left set", 0xc109_86a4, true),
        // **Not** tolerated. The air leaves both exactly as it found them.
        ("[28:24] cleared", 0x4009_86a4, false),
        ("eof cleared", 0x0109_86a4, false),
    ] {
        let got = run(Some(dw0));
        println!("{label:34} {dw0:#010x} -> {got:?}");
        assert_eq!(
            got.1, expect_rx,
            "{label}: the guest's answer is not what this phase measured"
        );
    }
    // `[23:12]` is *read* even though every value is tolerated: the
    // instruction counts differ. "Undetermined" is the honest word for it,
    // not "ignored".
    assert_ne!(
        run(Some(0x41a9_06a4)).0,
        delivered.0,
        "if these ever agree, the guest has stopped reading [23:12]"
    );
}

// ---------------------------------------------------------------------------
// M4 U1 — the TX-completion pass.
//
// The experiment `docs/debt/emu-c6-radio-tx-never-completes.md` asks for, run
// against **the state a real completion would find** rather than against the
// machine as it sits: M4 P2's lesson (Appendix B.5) is that a sweep with
// nothing to find measures the experiment. `test_espnow`'s own
// `[test_espnow] tx simulated_button … event=N` line and `--break-at
// lmacTxDone` are the two oracles.
// ---------------------------------------------------------------------------

/// The TX slot's PLCP0, for reading back the descriptor the blob armed.
const TX_PLCP0: u32 = 0x4d6c;

/// A machine run past its own arming write (~1,036 ms) with the air armed —
/// one frame handed to the MAC, the blob waiting for a completion.
fn a_machine_that_armed_a_frame(elf: &str, buf: Option<&SharedBuffer>) -> Esp32C6Machine {
    let mut m = build(elf, "a0:f2:62:87:b4:8c", buf, TxLogSink::Off);
    m.arm_air(ParticipantId(0));
    m.run_until(&until(ms(1_100)));
    assert!(
        m.usb_sj().text().contains("[test_espnow] radio ready"),
        "the radio never came up"
    );
    m
}

/// The descriptor PLCP0 points at, and a check that the go strobe is there —
/// i.e. that this machine really did arm a frame.
fn armed_descriptor(m: &mut Esp32C6Machine) -> u32 {
    let plcp0 = m.peek_word(WIFI_MAC + TX_PLCP0).expect("the PLCP0 word");
    assert_eq!(
        plcp0 & 0xc000_0000,
        0xc000_0000,
        "no go strobe in PLCP0 ({plcp0:#010x}): this machine armed nothing"
    );
    0x4080_0000 | (plcp0 & 0x000f_ffff)
}

fn raise_mac(m: &mut Esp32C6Machine, bits: u32) {
    let i = m
        .bus
        .peripheral_index("WIFI_MAC")
        .expect("the radio window");
    m.bus
        .with_peripheral::<lp_emu_esp32c6::periph::wifi_stub::WifiStub, _>(i, |w, _| {
            w.raise_event(bits)
        });
    m.bus
        .irq
        .set_level(lp_emu_esp32c6::regs::source::WIFI_MAC, true);
}

/// A trace line's `pc=0x…`, symbolized against the image.
fn pc_of(line: &str) -> Option<u32> {
    let rest = line.split("pc=0x").nth(1)?;
    u32::from_str_radix(rest.get(..8)?, 16).ok()
}

/// **Where the wedge is.** M4 P0 §3 measured that the guest stops making
/// progress after the arming write — no `wfi`, no MMIO, 160 M instructions a
/// second — and called it "a spin on RAM" without saying where. The frame
/// pointer chain says where.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn u1_where_the_guest_is_wedged() {
    let Some(elf) = espnow_elf() else { return };
    let mut m = build(&elf, "a0:f2:62:87:b4:8c", None, TxLogSink::Off);
    m.arm_air(ParticipantId(0));
    for at in [900u64, 1_030, 1_040, 1_100, 1_500, 2_000] {
        m.run_until(&until(ms(at)));
        println!("\n=== at {at} ms, {} instructions ===", m.instructions());
        for (i, (addr, sym)) in m.backtrace().into_iter().enumerate() {
            println!("  #{i:<2} {addr:#010x}  {sym}");
        }
        let regs = m.registers();
        let named = ["zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1"];
        for (i, v) in regs.iter().enumerate().take(18) {
            let name = named.get(i).copied().unwrap_or("");
            let sym = m.symbolize(*v).unwrap_or_default();
            println!("  x{i:<2} {name:<5} {v:#010x} {sym}");
        }
    }
}

/// One sweep candidate: a register to put `bits` in before the raise.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    block: &'static str,
    off: u32,
    bits: u32,
    state: TxState,
}

/// The registers this pass sweeps, and why each is a candidate:
///
/// - `WIFI_MAC+0x4c48` — the event word M4 P1 swept with an empty RX ring.
/// - `WIFI_MAC+0x4c34` — **never swept.** The ISR reads it immediately after
///   the event word (`hal_mac_interrupt_get_bsscolor+0x4`) and clears through
///   `+0x4c38`: the shape of a second event register, and P1 read its zero as
///   scenery rather than as a candidate.
/// - `WIFI_PWR+0x37b0` — the PWR block's event word.
/// - `WIFI_PWR+0x37ac` — read beside it by `pwr_hal_get_intr_raw_signal+0x4`
///   and never cleared: a raw-signal twin.
const SWEPT_REGISTERS: [(&str, u32); 4] = [
    ("WIFI_MAC", 0x4c48),
    ("WIFI_MAC", 0x4c34),
    ("WIFI_PWR", 0x37b0),
    ("WIFI_PWR", 0x37ac),
];

/// What one candidate did.
///
/// **The instruction count is not a discriminator on this path and must not
/// be read as one.** The wedged guest retires one instruction per cycle in
/// `send_channel`'s spin, so a run bounded by a cycle deadline retires the
/// same number whatever the ISR did with its tens of instructions — which is
/// also why M4 P1's "identical instruction count for all 64 candidates" was
/// never the strong evidence it read as. What separates the paths is the
/// number of radio-window accesses the raise produced and which of them are
/// not the ISR's staples.
#[derive(Debug)]
struct Row {
    candidate: Candidate,
    instructions: u64,
    /// Accesses to either radio block after the raise: the path length.
    accesses: usize,
    tx_done: bool,
    tx_line: bool,
    /// The first access to either radio block, after the raise, that is not
    /// one of the reads and writes the ISR always makes — the "next register
    /// read" the brief asks each row to name.
    novel: Option<String>,
}

/// The accesses every ISR entry makes, whatever it is told. Anything else is
/// a path this candidate opened.
const ISR_STAPLES: [&str; 7] = [
    "+0x4c48", "+0x4c34", "+0x37b0", "+0x37ac", "+0x4c4c", "+0x4c38", "+0x37b4",
];

/// What state to put the TX slot in before raising — "what a real completion
/// would find", enumerated rather than assumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TxState {
    /// Exactly as the blob left it: `owner` and `eof` set in the descriptor,
    /// the go strobe still in PLCP0.
    AsArmed,
    /// `owner` cleared on the TX descriptor — the `lldesc` convention for
    /// "the hardware has handed this back".
    OwnerClear,
    /// The go strobe cleared out of PLCP0 — "the queue is idle again".
    StrobeClear,
    /// Both.
    Both,
}

impl TxState {
    fn label(self) -> &'static str {
        match self {
            TxState::AsArmed => "as-armed",
            TxState::OwnerClear => "owner-clear",
            TxState::StrobeClear => "strobe-clear",
            TxState::Both => "both",
        }
    }
}

/// Run one candidate to the oracles, with the TX slot put into its state
/// first.
fn sweep_one(elf: &str, candidate: Candidate) -> Row {
    let state = candidate.state;
    let buf = SharedBuffer::new();
    let mut m = a_machine_that_armed_a_frame(elf, Some(&buf));
    let desc = armed_descriptor(&mut m);
    if matches!(state, TxState::OwnerClear | TxState::Both) {
        let dw0 = m.peek_word(desc).expect("the descriptor word");
        m.poke_word(desc, dw0 & !(1 << 31));
    }
    if matches!(state, TxState::StrobeClear | TxState::Both) {
        let plcp0 = m.peek_word(WIFI_MAC + TX_PLCP0).expect("PLCP0");
        m.poke_word(WIFI_MAC + TX_PLCP0, plcp0 & !0xc000_0000);
    }
    let _ = m.break_at("lmacTxDone");
    let mark = buf.lines().len();
    let (block, off) = (candidate.block, candidate.off);
    let i = m.bus.peripheral_index(block).expect("the block");
    m.bus
        .with_peripheral::<lp_emu_esp32c6::periph::wifi_stub::WifiStub, _>(i, |w, _| {
            w.raise_event(0)
        });
    let base = if block == "WIFI_MAC" {
        WIFI_MAC
    } else {
        0x600a_9900
    };
    m.poke_word(base + off, candidate.bits);
    m.bus
        .irq
        .set_level(lp_emu_esp32c6::regs::source::WIFI_MAC, true);
    let outcome = m.run_until(&until(ms(1_400)));
    let lines = buf.lines();
    let novel = lines
        .iter()
        .skip(mark)
        .find(|l| {
            (l.contains(" R4 ") || l.contains(" W4 ")) && !ISR_STAPLES.iter().any(|s| l.contains(s))
        })
        .map(|l| {
            let sym = pc_of(l).and_then(|pc| m.symbolize(pc)).unwrap_or_default();
            let short = l.split(" R4 ").last().unwrap_or(l);
            let short = short.split(" W4 ").last().unwrap_or(short);
            format!("{short}  pc={:#010x} {sym}", pc_of(l).unwrap_or(0))
        });
    let accesses = lines
        .iter()
        .skip(mark)
        .filter(|l| l.contains(" R4 ") || l.contains(" W4 "))
        .count();
    Row {
        candidate,
        instructions: m.instructions(),
        accesses,
        tx_done: matches!(outcome, Outcome::Breakpoint { .. }),
        tx_line: m
            .usb_sj()
            .text()
            .contains("[test_espnow] tx simulated_button"),
        novel,
    }
}

fn print_rows(rows: &[Row]) {
    println!(
        "{:<9} {:<8} {:<12} {:<13} {:>7}  {:<10} {:<7}  next-novel-access",
        "block", "off", "bits", "tx-state", "acc", "lmacTxDone", "tx-line"
    );
    for r in rows {
        println!(
            "{:<9} +{:#06x} {:#012x} {:<13} {:>7}  {:<10} {:<7}  {}",
            r.candidate.block,
            r.candidate.off,
            r.candidate.bits,
            r.candidate.state.label(),
            r.accesses,
            r.tx_done,
            r.tx_line,
            r.novel.as_deref().unwrap_or("-")
        );
    }
    let paths: std::collections::BTreeSet<usize> = rows.iter().map(|r| r.accesses).collect();
    let counts: std::collections::BTreeSet<u64> = rows.iter().map(|r| r.instructions).collect();
    println!(
        "{} candidates, {} distinct access counts {:?}, {} distinct instruction counts \
         (the latter measures the deadline, not the path)",
        rows.len(),
        paths.len(),
        paths,
        counts.len()
    );
}

/// **Method 2, the sweep.** One bit at a time in each of the four registers
/// the ISR touches, then the enable mask `hal_init` programmed
/// (`WIFI_MAC+0x4c40 = 0x19a879e0`) and its own bits, against both oracles.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn u1_sweep_every_event_bit_after_an_armed_tx() {
    let Some(elf) = espnow_elf() else { return };
    let mut rows = Vec::new();
    for (block, off) in SWEPT_REGISTERS {
        for bits in (0..32u32)
            .map(|n| 1u32 << n)
            .chain([0xffff_ffff, 0x19a8_79e0])
        {
            rows.push(sweep_one(
                &elf,
                Candidate {
                    block,
                    off,
                    bits,
                    state: TxState::AsArmed,
                },
            ));
        }
    }
    print_rows(&rows);
    let hits: Vec<&Row> = rows.iter().filter(|r| r.tx_done || r.tx_line).collect();
    println!("candidates that reached an oracle: {hits:?}");
}

/// **Method 3.** The same sweep of the MAC's event word, but with the TX slot
/// put into the state a *finished* transmission would leave it in — `owner`
/// handed back on the descriptor, the go strobe gone from PLCP0, or both.
///
/// M4 P2's lesson applied to the TX side: P1 swept an event word on a machine
/// whose TX slot still said "armed and busy", which is a machine with nothing
/// to find.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn u1_sweep_the_event_word_against_a_finished_tx_slot() {
    let Some(elf) = espnow_elf() else { return };
    let mut rows = Vec::new();
    for state in [TxState::OwnerClear, TxState::StrobeClear, TxState::Both] {
        for bits in (0..32u32)
            .map(|n| 1u32 << n)
            .chain([0xffff_ffff, 0x19a8_79e0])
        {
            rows.push(sweep_one(
                &elf,
                Candidate {
                    block: "WIFI_MAC",
                    off: 0x4c48,
                    bits,
                    state,
                },
            ));
        }
    }
    print_rows(&rows);
    let hits: Vec<&Row> = rows.iter().filter(|r| r.tx_done || r.tx_line).collect();
    println!("candidates that reached an oracle: {hits:?}");
}

/// **Why nothing else runs either.** M4 P0 §3 measured "no MMIO of any kind"
/// after the arming write and read it as a RAM spin with the timer interrupt
/// not being taken. This is the unfiltered version of that measurement,
/// bucketed by block, over the window either side of the arming write.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn u1_what_the_machine_touches_after_the_arming_write() {
    let Some(elf) = espnow_elf() else { return };
    let buf = SharedBuffer::new();
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf.clone().into()))
        .trace(Box::new(buf.clone()), vec![])
        .usb_host(UsbHost::Attached { draining: true })
        .efuse(EfuseIdentity {
            mac: EfuseIdentity::parse_mac("a0:f2:62:87:b4:8c").expect("a MAC"),
            ..EfuseIdentity::default()
        })
        .build()
        .expect("a machine");
    m.arm_air(ParticipantId(0));
    for (from, to) in [(1_000u64, 1_036u64), (1_036, 1_100), (1_100, 1_400)] {
        m.run_until(&until(ms(from)));
        let mark = buf.lines().len();
        let idle_before = m.idle_skips();
        m.run_until(&until(ms(to)));
        let lines = buf.lines();
        let mut by_block: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for line in lines.iter().skip(mark) {
            if !(line.contains(" R4 ") || line.contains(" W4 ")) {
                continue;
            }
            let block = line
                .split_whitespace()
                .find(|w| w.contains('+') && w.contains("0x"))
                .unwrap_or("?")
                .split('+')
                .next()
                .unwrap_or("?")
                .to_string();
            *by_block.entry(block).or_default() += 1;
        }
        println!(
            "\n{from}–{to} ms: {} accesses, wfi skips {} -> {}, {by_block:?}",
            lines.len() - mark,
            idle_before,
            m.idle_skips()
        );
        let tail: Vec<&String> = lines
            .iter()
            .skip(mark)
            .filter(|l| l.contains(" R4 ") || l.contains(" W4 "))
            .collect();
        for line in tail.iter().rev().take(30).rev() {
            let sym = pc_of(line)
                .and_then(|pc| m.symbolize(pc))
                .unwrap_or_default();
            println!("  {line}   {sym}");
        }
    }
}

/// The paths the sweep separated, printed in full. The interesting one is
/// `WIFI_PWR+0x37b0` bit 3: it is the only candidate in 132 that takes the
/// ISR into a **TX-queue** function (`hal_pm_unblock_txq`).
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn u1_the_paths_the_event_words_open() {
    let Some(elf) = espnow_elf() else { return };
    for (block, off, bits) in [
        ("WIFI_PWR", 0x37b0u32, 1u32 << 3),
        ("WIFI_PWR", 0x37b0, 1 << 4),
        ("WIFI_PWR", 0x37b0, 0xffff_ffff),
        ("WIFI_MAC", 0x4c48, 1 << 14),
    ] {
        let buf = SharedBuffer::new();
        let mut m = a_machine_that_armed_a_frame(&elf, Some(&buf));
        let desc = armed_descriptor(&mut m);
        println!(
            "\n=== {block}+{off:#06x} = {bits:#010x}, descriptor {desc:#010x} dw0={:#010x} ===",
            m.peek_word(desc).unwrap_or(0)
        );
        let _ = m.break_at("lmacTxDone");
        let mark = buf.lines().len();
        let base = if block == "WIFI_MAC" {
            WIFI_MAC
        } else {
            0x600a_9900
        };
        m.poke_word(base + off, bits);
        m.bus
            .irq
            .set_level(lp_emu_esp32c6::regs::source::WIFI_MAC, true);
        let outcome = m.run_until(&until(ms(1_400)));
        let lines = buf.lines();
        for line in lines
            .iter()
            .skip(mark)
            .filter(|l| l.contains(" R4 ") || l.contains(" W4 "))
            .take(48)
        {
            let sym = pc_of(line)
                .and_then(|pc| m.symbolize(pc))
                .unwrap_or_default();
            println!("  {line}   {sym}");
        }
        println!("  outcome={outcome:?}, console={:?}", m.usb_sj().text());
    }
}

/// **The gate the sweep found.** Event bits 7, 8 and 19 of
/// `WIFI_MAC+0x4c48` are the only ones in 132 that take the ISR into
/// `hal_mac_get_txq_state` (`0x40806382`), which reads `WIFI_MAC+0x4cb0` and
/// `+0x4cb8`, finds zero and returns. This answers those two with every
/// candidate value and asks the oracles again.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn u1_answer_the_txq_state_registers() {
    let Some(elf) = espnow_elf() else { return };
    let mut rows: Vec<(u32, String, u32, usize, bool, bool, Option<String>)> = Vec::new();
    for event in [1u32 << 7, 1 << 8, 1 << 19, 0x19a8_79e0, 0xffff_ffff] {
        for (label, a, b) in [
            ("+0x4cb0 all ones", 0xffff_ffffu32, 0u32),
            ("+0x4cb8 all ones", 0, 0xffff_ffff),
            ("both all ones", 0xffff_ffff, 0xffff_ffff),
            ("both = 1", 1, 1),
            ("both = 0x10", 0x10, 0x10),
        ] {
            let buf = SharedBuffer::new();
            let mut m = a_machine_that_armed_a_frame(&elf, Some(&buf));
            let desc = armed_descriptor(&mut m);
            let dw0 = m.peek_word(desc).expect("dw0");
            m.poke_word(desc, dw0 & !(1 << 31));
            let _ = m.break_at("lmacTxDone");
            let mark = buf.lines().len();
            m.poke_word(WIFI_MAC + 0x4cb0, a);
            m.poke_word(WIFI_MAC + 0x4cb8, b);
            m.poke_word(WIFI_MAC + 0x4c48, event);
            m.bus
                .irq
                .set_level(lp_emu_esp32c6::regs::source::WIFI_MAC, true);
            let outcome = m.run_until(&until(ms(1_400)));
            let lines = buf.lines();
            let accesses = lines
                .iter()
                .skip(mark)
                .filter(|l| l.contains(" R4 ") || l.contains(" W4 "))
                .count();
            let novel = lines
                .iter()
                .skip(mark)
                .filter(|l| l.contains(" R4 ") || l.contains(" W4 "))
                .find(|l| {
                    !ISR_STAPLES.iter().any(|s| l.contains(s))
                        && !l.contains("+0x4cb0")
                        && !l.contains("+0x4cb8")
                })
                .map(|l| {
                    let sym = pc_of(l).and_then(|pc| m.symbolize(pc)).unwrap_or_default();
                    format!("{} {sym}", l.split(" bb = ").next().unwrap_or(l))
                });
            rows.push((
                event,
                label.to_string(),
                a,
                accesses,
                matches!(outcome, Outcome::Breakpoint { .. }),
                m.usb_sj()
                    .text()
                    .contains("[test_espnow] tx simulated_button"),
                novel,
            ));
        }
    }
    println!(
        "{:<12} {:<18} {:>7}  {:<10} {:<7}  next-novel-access",
        "event", "txq-state", "acc", "lmacTxDone", "tx-line"
    );
    for (event, label, _, acc, done, line, novel) in &rows {
        println!(
            "{event:#012x} {label:<18} {acc:>7}  {done:<10} {line:<7}  {}",
            novel.as_deref().unwrap_or("-")
        );
    }
}

/// **U1's gate.** A machine on an air completes its transmissions, so the
/// `test_espnow` guest's own `send` returns and it goes round its loop:
/// `event=1`, then `event=2` a second later. Before this it printed neither.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn a_tx_completes_and_the_guest_sends_again() {
    let Some(elf) = espnow_elf() else { return };
    let mut m = machine(&elf, "a0:f2:62:87:b4:8c");
    m.arm_air(ParticipantId(0));
    m.run_until(&until(ms(2_400)));
    let console = m.usb_sj().text();
    println!("{console}");
    println!(
        "{} instructions, {} wfi skips",
        m.instructions(),
        m.idle_skips()
    );
    assert!(
        console.contains("[test_espnow] tx simulated_button device= event=1"),
        "the first send never returned:\n{console}"
    );
    assert!(
        console.contains("[test_espnow] tx simulated_button device= event=2"),
        "the guest sent once and stopped — a completion that does not repeat \
         is a one-off, not a model:\n{console}"
    );
    assert!(
        !console.contains("tx failed"),
        "the send returned an error:\n{console}"
    );
}

/// The off switch, restated for the completion: a machine that is **not** on
/// an air still never completes a TX, and retires the instruction count every
/// C6 gate and transcript in the tree was recorded against.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn a_machine_not_in_an_air_still_never_completes_a_tx() {
    let Some(elf) = espnow_elf() else { return };
    let mut plain = machine(&elf, "a0:f2:62:87:b4:8c");
    plain.run_until(&until(ms(1_500)));
    assert_eq!(plain.instructions(), 79_871_852, "M4 P0 §7's figure");
    assert!(
        !plain.usb_sj().text().contains("tx simulated_button"),
        "a machine nobody asked for radio behaviour from completed a TX"
    );
}

/// The chase, past `lmacTxDone`: event bit 7, `+0x4cb8` bit 0, and whatever
/// further registers the run below says are read next, answered one at a time
/// until the guest's own `tx simulated_button` line appears or the trace stops
/// saying anything new.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn u1_chase_the_completion_past_lmac_tx_done() {
    let Some(elf) = espnow_elf() else { return };
    let extra: Vec<(u32, u32)> = std::env::var("LP_U1_ANSWERS")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|pair| {
            let (off, value) = pair.split_once('=').expect("off=value");
            (
                u32::from_str_radix(off.trim_start_matches("0x"), 16).expect("an offset"),
                u32::from_str_radix(value.trim_start_matches("0x"), 16).expect("a value"),
            )
        })
        .collect();
    println!("extra answers: {extra:x?}");
    let buf = SharedBuffer::new();
    let mut m = a_machine_that_armed_a_frame(&elf, Some(&buf));
    let desc = armed_descriptor(&mut m);
    let dw0 = m.peek_word(desc).expect("dw0");
    m.poke_word(desc, dw0 & !(1 << 31));
    let mark = buf.lines().len();
    for (off, value) in &extra {
        m.poke_word(WIFI_MAC + off, *value);
    }
    m.poke_word(WIFI_MAC + 0x4cb8, 1);
    m.poke_word(WIFI_MAC + 0x4c48, 1 << 7);
    m.bus
        .irq
        .set_level(lp_emu_esp32c6::regs::source::WIFI_MAC, true);
    let outcome = m.run_until(&until(ms(2_400)));
    let lines = buf.lines();
    let after: Vec<&String> = lines
        .iter()
        .skip(mark)
        .filter(|l| l.contains(" R4 ") || l.contains(" W4 "))
        .collect();
    for line in after.iter().take(70) {
        let sym = pc_of(line)
            .and_then(|pc| m.symbolize(pc))
            .unwrap_or_default();
        println!("  {line}   {sym}");
    }
    println!(
        "{} accesses, outcome={outcome:?}\nconsole:\n{}",
        after.len(),
        m.usb_sj().text()
    );
}

/// **Method 1.** What the blob's ISR reads when source 0 is raised on a
/// machine that has *armed a frame*, with the TX descriptor as the blob left
/// it and with `owner` cleared — the `lldesc` convention for "the hardware is
/// done with this one".
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn u1_what_the_isr_reads_after_an_armed_tx() {
    let Some(elf) = espnow_elf() else { return };
    for (label, clear_owner) in [("as armed", false), ("owner cleared", true)] {
        let buf = SharedBuffer::new();
        let mut m = a_machine_that_armed_a_frame(&elf, Some(&buf));
        let desc = armed_descriptor(&mut m);
        let dw0 = m.peek_word(desc).expect("the descriptor word");
        println!("\n=== {label}: descriptor {desc:#010x} dw0={dw0:#010x} ===");
        if clear_owner {
            m.poke_word(desc, dw0 & !(1 << 31));
        }
        let before = m.instructions();
        let mark = buf.lines().len();
        let _ = m.break_at("lmacTxDone");
        raise_mac(&mut m, 0xffff_ffff);
        let outcome = m.run_until(&until(ms(1_120)));
        let lines = buf.lines();
        for line in lines.iter().skip(mark).take(60) {
            let sym = pc_of(line)
                .and_then(|pc| m.symbolize(pc))
                .unwrap_or_default();
            println!("{line}   {sym}");
        }
        println!(
            "{label}: {} lines, {} instructions, outcome={outcome:?}",
            lines.len() - mark,
            m.instructions() - before
        );
        println!("console: {:?}", m.usb_sj().text());
    }
}
