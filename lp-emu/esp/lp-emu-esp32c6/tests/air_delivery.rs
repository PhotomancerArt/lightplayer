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
/// # What this gate is, and what it is not
///
/// The milestone asked for "each prints the other's `rx` lines for at least
/// three consecutive events". **That is out of reach on this image and no
/// amount of delivery will bring it into reach**, for a reason that is
/// nothing to do with the air: the `test_espnow` guest arms exactly one
/// frame and then never returns from its own `send`
/// (`docs/debt/emu-c6-radio-tx-never-completes.md`). One frame per machine,
/// ever. And the machine that sends *second* is always sending into a
/// machine that has already stopped draining, so the traffic is one-way.
///
/// So the gate is the one the director's dispatch named: **the one frame the
/// blob does send is delivered, and the receiving guest prints it.** The
/// pair is staggered ([`Lockstep::stagger`]) because two identical images
/// started in the same cycle wedge in the same cycle — see that method's
/// docs, and `the_symmetric_pair_delivers_but_cannot_print` below, which
/// pins the failure so nobody has to rediscover it.
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

    // Both armed their one frame; the air carried both; only the machine
    // that was still draining printed one.
    assert_eq!(report.machines[0].frames_sent, 1);
    assert_eq!(report.machines[1].frames_sent, 1);
    assert_eq!(pair.machine(second).expect("b").air_frames_delivered(), 1);
    assert!(
        console_b.contains("[test_espnow] rx "),
        "the receiving guest never printed an rx line:\n{console_b}"
    );
    assert!(
        !console_a.contains("[test_espnow] rx "),
        "machine 0 was wedged in its own send and cannot have drained one"
    );

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

/// The failure the stagger exists for, pinned: two identical images started
/// in the same cycle both **deliver** and neither **prints**, because each
/// is wedged inside its own `send` by the time the other's frame arrives.
///
/// If this ever starts printing, a TX has completed and
/// `docs/debt/emu-c6-radio-tx-never-completes.md` is closed — which would be
/// very good news and should be chased, not silenced.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn the_symmetric_pair_delivers_but_cannot_print() {
    let Some(elf) = espnow_elf() else { return };
    let a = machine(&elf, "a0:f2:62:87:b4:8c");
    let b = machine(&elf, "a0:f2:62:85:a8:7c");
    let mut pair = Lockstep::new(vec![a, b]).expect("a pair");
    let report = pair.run_until(ms(1_200), &StopCondition::default());
    for i in 0..2 {
        let m = pair.machine(ParticipantId(i)).expect("a machine");
        assert_eq!(report.machines[i].frames_sent, 1);
        assert_eq!(m.air_frames_delivered(), 1, "the ring took it");
        assert!(
            !m.usb_sj().text().contains("[test_espnow] rx "),
            "machine {i} printed an rx line — has a TX completed?"
        );
    }
    println!("both delivered, neither printed: the wedge, not the air");
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
