//! M4 P2's gate: a frame delivered into a running guest's RX ring, and what
//! the guest did with it.
//!
//! `#[ignore]`d and driven by **`LP_EMU_C6_ESPNOW_ELF`** — a path to an
//! ELF built from `lp-fw/fw-esp32c6` with
//! `--no-default-features --features test_espnow,esp32c6`. That image is not
//! one of `test_support`'s named artefacts and every feature set of
//! `fw-esp32c6` builds to the same path, so this test takes the path it is
//! given and never builds or guesses one. Without the variable it skips.
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
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, Esp32C6Machine, StopCondition, UsbHost};
use lp_emu_esp32c6::memmap;

/// The 56-byte ESP-NOW broadcast frame the `test_espnow` image arms, read out
/// of guest RAM by `--tx-log` on a machine whose eFuse MAC is the bench's
/// second board (`a0:f2:62:85:a8:7c`). Byte for byte what the sender armed.
const FRAME_FROM_THE_OTHER_BOARD: &str = "d0000000ffffffffffffa0f26285a87cffffffffffff00007f18fe34\
e2b3830ddd1618fe340402504c01016285a87c000000000100000000";

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
    traced_machine(elf, mac, None)
}

fn traced_machine(elf: &str, mac: &str, buf: Option<&SharedBuffer>) -> Esp32C6Machine {
    let mut b = Esp32C6Builder::new().app(AppSource::Path(elf.into()));
    if let Some(buf) = buf {
        b = b.trace(
            Box::new(buf.clone()),
            vec!["WIFI_MAC".to_string(), "WIFI_PWR".to_string()],
        );
    }
    b
        .usb_host(UsbHost::Attached { draining: true })
        .efuse(EfuseIdentity {
            mac: EfuseIdentity::parse_mac(mac).expect("a MAC"),
            ..EfuseIdentity::default()
        })
        .build()
        .expect("the test_espnow image builds a machine")
}

/// What the guest's ISR does with a delivery: every `WIFI_MAC`/`WIFI_PWR`
/// access in the window after one, printed.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn what_the_isr_reads_after_a_delivery() {
    let Some(elf) = espnow_elf() else { return };
    let buf = SharedBuffer::new();
    let mut m = traced_machine(&elf, "a0:f2:62:87:b4:8c", Some(&buf));
    m.arm_air(ParticipantId(0));
    m.run_until(&StopCondition {
        stop_cycle: Some(ms(500)),
        ..StopCondition::default()
    });
    let mark = buf.lines().len();
    let bytes = hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD);
    m.offer_air_frame(&AirFrame {
        from: ParticipantId(1),
        at: ms(500) - 1,
        due: ms(500),
        bytes,
    });
    m.run_until(&StopCondition {
        stop_cycle: Some(ms(520)),
        ..StopCondition::default()
    });
    let lines = buf.lines();
    println!(
        "--- {} radio accesses after the delivery ---",
        lines.len() - mark
    );
    for line in lines.iter().skip(mark).take(200) {
        println!("{line}");
    }
    println!("--- console ---");
    println!("{}", m.usb_sj().text());
}

fn ms(n: u64) -> u64 {
    n * 1_000 * memmap::CYCLES_PER_US
}

/// One frame, into a guest that is still running its tick loop.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn one_frame_reaches_the_apps_own_rx_line() {
    let Some(elf) = espnow_elf() else { return };
    let mut m = machine(&elf, "a0:f2:62:87:b4:8c");
    m.arm_air(ParticipantId(0));

    // 500 ms: the radio is up (the ready line is out at ~36 ms) and the app
    // is in its 100 ms tick loop, four ticks before it tries to send.
    m.run_until(&StopCondition {
        stop_cycle: Some(ms(500)),
        ..StopCondition::default()
    });
    let before = m.usb_sj().text();
    assert!(
        before.contains("[test_espnow] radio ready"),
        "the radio never came up: {before}"
    );

    let bytes = hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD);
    m.offer_air_frame(&AirFrame {
        from: ParticipantId(1),
        at: ms(500) - 1,
        due: ms(500),
        bytes: bytes.clone(),
    });
    println!(
        "offered={} delivered={} undelivered={}",
        m.air_frames_offered(),
        m.air_frames_delivered(),
        m.air_frames_undelivered()
    );
    assert_eq!(m.air_frames_delivered(), 1, "the ring took it");

    m.run_until(&StopCondition {
        stop_cycle: Some(ms(1_000)),
        ..StopCondition::default()
    });
    let after = m.usb_sj().text();
    println!("--- console ---\n{after}\n--- end ---");
    println!("frame sent  : {}", bytes_to_hex(&bytes));
    assert!(
        after.contains("[test_espnow] rx "),
        "the guest never printed an rx line"
    );
}

/// The sweep M4 P1 ran against the TX oracle, run again against the **RX**
/// one with a frame already in the ring: every bit of the MAC's event word,
/// one at a time, plus the enable mask the blob programmed.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn every_event_bit_against_the_rx_oracle() {
    let Some(elf) = espnow_elf() else { return };
    let mut rows = Vec::new();
    for bits in (0..32u32)
        .map(|n| 1u32 << n)
        .chain([0xffff_ffff, 0x19a8_79e0, 0])
    {
        let buf = SharedBuffer::new();
        let mut m = traced_machine(&elf, "a0:f2:62:87:b4:8c", Some(&buf));
        m.arm_air(ParticipantId(0));
        let i = m
            .bus
            .peripheral_index("WIFI_MAC")
            .expect("the radio window is on the decode table");
        m.bus
            .with_peripheral::<lp_emu_esp32c6::periph::wifi_stub::WifiStub, _>(i, |w, _| {
                w.set_rx_event_bits(bits)
            });
        m.run_until(&StopCondition {
            stop_cycle: Some(ms(500)),
            ..StopCondition::default()
        });
        let mark = buf.lines().len();
        m.offer_air_frame(&AirFrame {
            from: ParticipantId(1),
            at: ms(500) - 1,
            due: ms(500),
            bytes: hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD),
        });
        m.run_until(&StopCondition {
            stop_cycle: Some(ms(700)),
            ..StopCondition::default()
        });
        let lines = buf.lines();
        let after: Vec<&String> = lines.iter().skip(mark).collect();
        let rx = m.usb_sj().text().contains("[test_espnow] rx ");
        rows.push((
            bits,
            m.instructions(),
            after.len(),
            rx,
            after
                .iter()
                .any(|l| l.contains("+0x4088") || l.contains("+0x408c") || l.contains("+0x4090")),
        ));
    }
    println!("bits        instructions  radio-accesses  rx-line  cursor-read");
    for (bits, ins, n, rx, cursor) in &rows {
        println!("{bits:#010x}  {ins:>12}  {n:>14}  {rx:>7}  {cursor:>11}");
    }
    let distinct: std::collections::BTreeSet<u64> = rows.iter().map(|r| r.1).collect();
    println!(
        "distinct instruction counts across {} candidates: {}",
        rows.len(),
        distinct.len()
    );
}

/// One candidate, in detail: every distinct radio offset the guest touched
/// after the delivery, with the first value and the writer's PC.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn one_candidate_in_detail() {
    let Some(elf) = espnow_elf() else { return };
    let bits = u32::from_str_radix(
        std::env::var("LP_EMU_C6_RX_BITS")
            .unwrap_or_else(|_| "4000".into())
            .trim_start_matches("0x"),
        16,
    )
    .expect("hex");
    let buf = SharedBuffer::new();
    let mut m = traced_machine(&elf, "a0:f2:62:87:b4:8c", Some(&buf));
    m.arm_air(ParticipantId(0));
    let i = m.bus.peripheral_index("WIFI_MAC").expect("radio window");
    m.bus
        .with_peripheral::<lp_emu_esp32c6::periph::wifi_stub::WifiStub, _>(i, |w, _| {
            w.set_rx_event_bits(bits)
        });
    m.run_until(&StopCondition {
        stop_cycle: Some(ms(500)),
        ..StopCondition::default()
    });
    let mark = buf.lines().len();
    m.offer_air_frame(&AirFrame {
        from: ParticipantId(1),
        at: ms(500) - 1,
        due: ms(500),
        bytes: hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD),
    });
    m.run_until(&StopCondition {
        stop_cycle: Some(ms(505)),
        ..StopCondition::default()
    });
    let lines = buf.lines();
    println!("bits={bits:#010x}; {} accesses", lines.len() - mark);
    let mut seen = std::collections::BTreeSet::new();
    let mut shown = 0;
    for line in lines.iter().skip(mark) {
        let key: String = line.chars().skip_while(|c| *c != ' ').collect::<String>()
            [..]
            .split(" = ")
            .next()
            .unwrap_or("")
            .to_string();
        if seen.insert(key) {
            println!("{line}");
            shown += 1;
            if shown > 80 {
                break;
            }
        }
    }
    println!("--- first 40 lines verbatim ---");
    for line in lines.iter().skip(mark).take(40) {
        println!("{line}");
    }
    println!("--- console ---");
    println!("{}", m.usb_sj().text());
}

/// The absurd-value test, byte by byte: which bytes of the header the guest
/// actually reads. A byte whose value the guest never notices is a byte we
/// invented.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn which_header_bytes_the_guest_notices() {
    let Some(elf) = espnow_elf() else { return };
    const WIFI_MAC: u32 = 0x600a_0000;
    let run = |poke: Option<(u32, u8)>| -> (u64, usize) {
        let buf = SharedBuffer::new();
        let mut m = traced_machine(&elf, "a0:f2:62:87:b4:8c", Some(&buf));
        m.arm_air(ParticipantId(0));
        m.run_until(&StopCondition {
            stop_cycle: Some(ms(500)),
            ..StopCondition::default()
        });
        let mark = buf.lines().len();
        m.offer_air_frame(&AirFrame {
            from: ParticipantId(1),
            at: ms(500) - 1,
            due: ms(500),
            bytes: hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD),
        });
        if let Some((byte, value)) = poke {
            let base = m.peek_word(WIFI_MAC + 0x4084).unwrap_or(0);
            let bufaddr = m.peek_word(base + 4).unwrap_or(0);
            let word_at = bufaddr + (byte & !3);
            let old = m.peek_word(word_at).unwrap_or(0);
            let mut b = old.to_le_bytes();
            b[(byte & 3) as usize] = value;
            m.poke_word(word_at, u32::from_le_bytes(b));
        }
        m.run_until(&StopCondition {
            stop_cycle: Some(ms(800)),
            ..StopCondition::default()
        });
        (m.instructions(), buf.lines().len() - mark)
    };
    let baseline = run(None);
    println!("baseline instructions={} accesses={}", baseline.0, baseline.1);
    let mut noticed = Vec::new();
    for byte in 0..92u32 {
        let a = run(Some((byte, 0xff)));
        if a != baseline {
            noticed.push((byte, a));
        }
    }
    println!("bytes whose value the guest noticed (0xff): {noticed:?}");
}

/// Which byte of the header, at which value, gets the frame past
/// `wDev_ProcessRxSucData` and into `ppRxPkt`.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn which_header_byte_gets_the_frame_into_pprxpkt() {
    let Some(elf) = espnow_elf() else { return };
    const WIFI_MAC: u32 = 0x600a_0000;
    let byte: u32 = std::env::var("LP_EMU_C6_HDR_BYTE")
        .unwrap_or_else(|_| "8".into())
        .parse()
        .expect("a byte offset");
    let mut hits = Vec::new();
    for value in 0..=255u32 {
        let mut m = machine(&elf, "a0:f2:62:87:b4:8c");
        m.arm_air(ParticipantId(0));
        m.run_until(&StopCondition {
            stop_cycle: Some(ms(500)),
            ..StopCondition::default()
        });
        let _ = m.break_at("ppRxPkt");
        m.offer_air_frame(&AirFrame {
            from: ParticipantId(1),
            at: ms(500) - 1,
            due: ms(500),
            bytes: hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD),
        });
        let base = m.peek_word(WIFI_MAC + 0x4084).unwrap_or(0);
        let buf = m.peek_word(base + 4).unwrap_or(0);
        let word_at = buf + (byte & !3);
        let lane = byte & 3;
        let old = m.peek_word(word_at).unwrap_or(0);
        let mut b = old.to_le_bytes();
        b[lane as usize] = value as u8;
        m.poke_word(word_at, u32::from_le_bytes(b));
        let outcome = m.run_until(&StopCondition {
            stop_cycle: Some(ms(700)),
            ..StopCondition::default()
        });
        if matches!(outcome, lp_emu_esp32c6::machine::Outcome::Breakpoint { .. }) {
            hits.push(value);
        }
    }
    println!("header byte {byte}: values that reach ppRxPkt = {hits:?}");
}

/// How far up the blob's RX chain a delivery gets: one breakpoint per
/// candidate symbol, each in its own run.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn how_far_up_the_rx_chain_a_delivery_gets() {
    let Some(elf) = espnow_elf() else { return };
    for sym in [
        "wDev_ProcessRxSucData",
        "wdevProcessRxSucDataAll",
        "lmacProcessRxSucData",
        "ppRxPkt",
        "ppProcessRxPktHdr",
        "ppEnqueueRxq",
        "wDev_AppendRxBlocks",
        "wdev_record_rx_linked_list",
        "wDev_Rxbuf_Init",
        "esp_now_recv_cb",
        "wdev_is_data_in_rxlist",
        "hal_mac_rx_read_rxdscrnext",
        "hal_mac_rx_get_last_dscr",
    ] {
        let mut m = machine(&elf, "a0:f2:62:87:b4:8c");
        m.arm_air(ParticipantId(0));
        m.run_until(&StopCondition {
            stop_cycle: Some(ms(500)),
            ..StopCondition::default()
        });
        let resolved = m.break_at(sym);
        m.offer_air_frame(&AirFrame {
            from: ParticipantId(1),
            at: ms(500) - 1,
            due: ms(500),
            bytes: hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD),
        });
        let outcome = m.run_until(&StopCondition {
            stop_cycle: Some(ms(800)),
            ..StopCondition::default()
        });
        println!(
            "{sym:28} resolved={:?} outcome={}",
            resolved.map(|a| format!("{a:#010x}")).ok(),
            match outcome {
                lp_emu_esp32c6::machine::Outcome::Breakpoint { cycle, .. } =>
                    format!("ENTERED at cyc={cycle}"),
                other => format!("{other:?}"),
            }
        );
    }
}

/// A search over what the filled descriptor's first word should say.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn which_descriptor_word_the_guest_accepts() {
    let Some(elf) = espnow_elf() else { return };
    const WIFI_MAC: u32 = 0x600a_0000;
    for (label, dw0) in [
        ("as delivered", None),
        ("no eof", Some(0x0109_86a4u32)),
        ("len untouched", Some(0x41a9_06a4)),
        ("owner kept", Some(0xc109_86a4)),
        ("offset cleared", Some(0x4009_86a4)),
        ("len in [11:0]", Some(0x41a9_0098)),
        ("size kept, len 0", Some(0x4100_06a4)),
        ("all but owner", Some(0x7fff_ffff)),
    ] {
        let buf = SharedBuffer::new();
        let mut m = traced_machine(&elf, "a0:f2:62:87:b4:8c", Some(&buf));
        m.arm_air(ParticipantId(0));
        m.run_until(&StopCondition {
            stop_cycle: Some(ms(500)),
            ..StopCondition::default()
        });
        let mark = buf.lines().len();
        m.offer_air_frame(&AirFrame {
            from: ParticipantId(1),
            at: ms(500) - 1,
            due: ms(500),
            bytes: hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD),
        });
        let base = m.peek_word(WIFI_MAC + 0x4084).unwrap_or(0);
        let seen = m.peek_word(base).unwrap_or(0);
        if let Some(dw0) = dw0 {
            m.poke_word(base, dw0);
        }
        m.run_until(&StopCondition {
            stop_cycle: Some(ms(800)),
            ..StopCondition::default()
        });
        println!(
            "{label:16} dw0={:#010x} accesses={:>6} instructions={:>10} rx={}",
            dw0.unwrap_or(seen),
            buf.lines().len() - mark,
            m.instructions(),
            m.usb_sj().text().contains("[test_espnow] rx ")
        );
    }
}

/// A small search over the registers the RX path reads beside the ring base.
#[test]
#[ignore = "needs a test_espnow ELF in LP_EMU_C6_ESPNOW_ELF"]
fn which_rx_cursor_register_opens_the_happy_path() {
    let Some(elf) = espnow_elf() else { return };
    const WIFI_MAC: u32 = 0x600a_0000;
    let candidates: Vec<(&str, u32, u32)> = vec![
        ("baseline", 0, 0),
        ("0x408c = next", 0x408c, 0),
        ("0x4c70 = 2", 0x4c70, 2),
        ("0x4c70 = desc", 0x4c70, 1),
        ("0x4088 = desc", 0x4088, 1),
        ("0x4090 = desc", 0x4090, 1),
        ("0x4094 = desc", 0x4094, 1),
        ("0x4088 = next", 0x4088, 0),
        ("0x4090 = next", 0x4090, 0),
        ("0x4094 = next", 0x4094, 0),
    ];
    for (label, off, kind) in candidates {
        let buf = SharedBuffer::new();
        let mut m = traced_machine(&elf, "a0:f2:62:87:b4:8c", Some(&buf));
        m.arm_air(ParticipantId(0));
        m.run_until(&StopCondition {
            stop_cycle: Some(ms(500)),
            ..StopCondition::default()
        });
        let mark = buf.lines().len();
        m.offer_air_frame(&AirFrame {
            from: ParticipantId(1),
            at: ms(500) - 1,
            due: ms(500),
            bytes: hex_to_bytes(FRAME_FROM_THE_OTHER_BOARD),
        });
        let base = m.peek_word(WIFI_MAC + 0x4084).unwrap_or(0);
        if off != 0 {
            let value = match kind {
                0 => base + 12,
                1 => base,
                other => other,
            };
            m.poke_word(WIFI_MAC + off, value);
        }
        m.run_until(&StopCondition {
            stop_cycle: Some(ms(800)),
            ..StopCondition::default()
        });
        let n = buf.lines().len() - mark;
        let text = m.usb_sj().text();
        println!(
            "{label:16} accesses={n:>7} instructions={:>10} rx={}",
            m.instructions(),
            text.contains("[test_espnow] rx ")
        );
    }
}
