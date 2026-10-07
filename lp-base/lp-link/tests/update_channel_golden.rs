//! The update channel's bytes, pinned. Channel 3 carries the over-the-air
//! update protocol (`lpc-update`, protocol v1): once update-capable cores
//! are fielded, a future host reaches them over exactly this link, so —
//! like `plain_bytes_golden.rs` — this is a **never break** pin, not a
//! "bump the proto" one (OTA plan, QY1 decided yes as N5; ADR 2).
//!
//! Each transcript is a fixed exchange between two links of one preset with
//! fixed nonces: the handshake, a reliable channel-3 message each way (a
//! query and a manifest-shaped answer), a fragmented chunk, their ACKs, and
//! keepalives, every frame in the order it went, as hex. A mismatch is a
//! break of the link a fielded core keeps: never re-capture one to make this
//! pass.
//!
//! - `usb()` (Stream framing, COBS): captured once, when channel 3 joined
//!   `usb()`'s reliable mask;
//! - `ble()` (Datagram framing: each line is one frame, one GATT write or
//!   notification, no COBS): captured once, when core-only began serving
//!   channel 3 over Bluetooth (OTA Part C, QY1/N5's link freeze reaching
//!   BLE). The board cuts its own buffers and fits `max_payload` to the
//!   connection's MTU (`fw-esp32-common`'s `radio_link_config`), and opens
//!   core-only's links with a wider receive window: values a SYN carries,
//!   not the frame format pinned here.

use lp_link::{CH_UPDATE, Framing, Link, LinkConfig, LinkState, Micros, SelectiveRepeat};

#[test]
fn usb_update_channel_bytes_are_unchanged() {
    check(&transcript(LinkConfig::usb()), USB);
}

#[test]
fn ble_update_channel_bytes_are_unchanged() {
    check(&transcript(LinkConfig::ble()), BLE);
}

#[test]
fn channel_three_is_reliable_on_every_board_preset() {
    for (name, cfg) in [
        ("usb", LinkConfig::usb()),
        ("uart", LinkConfig::uart()),
        ("ble", LinkConfig::ble()),
        // The LAN link (Wi-Fi roadmap M6): hosts use `ws()` itself, and the
        // board's `lan_link_config()` is cut from it (fw-esp32-common pins
        // that its cut keeps channel 3).
        ("ws", LinkConfig::ws()),
    ] {
        assert!(cfg.is_reliable(CH_UPDATE), "{name}() carries channel 3");
    }
}

const USB: &[&str] = &[
    "A 00020301010511111111010101010107010804949ced00",
    "B 0002030101092222222211111111010701084e90312300",
    "A 00020301010a11111111222222220107010807a70e7700",
    "A 000278010808510182e8ccef00",
    "B 0002782501084d7b2270726f746f223a312c227374617465223a2272756e6e696e67227d54171af300",
    "A 0007700101084445021001930b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe0123486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799ec3e80d32577ca1c6eb10355a7fa4c9ee13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db54254a6f94b9de03284d7297bce1062b50759abfe4092e53789dc2e70c31567ba0c5ea0f34597ea3c8ed12375c81a6cbf0153a5f84a9cef3183d6287acd1f61b40658aafd4f91e43688db2d7fc21466b90b5dafe1a24496e93b8dd02274c7196bbe0052a4f7499bee308dcb2150d00",
    "A 009d600201082d52779cc1e60b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe0123486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799ec3e80d32577ca1c6eb10355a7fa4c9ee13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db54254a6f94b9de03284d7297bce1062b50759abfe4092e53789dc2e70c31567ba0c5ea0f34597ea3c8ed12375c81a6cbf0153a5f84a9cef3183d6287acd1f61b40658aafd4f91e43688db2d7fc21466b90b5dafe1a24496e93b8dd02274c7196bbe0052a4f7499bee308fa77be4900",
    "B 0002020703082df28f3900",
    "A 0068680301082d52779cc1e60b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe0123486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799e0db4bfcb00",
    "B 000202070408683be24300",
    "A 000202070108c3c2ca1e00",
    "B 000202070408683be24300",
    "A 000202070108c3c2ca1e00",
    "B 000202070408683be24300",
];

const BLE: &[&str] = &[
    "A 03000000111111110000000000b40008b459217a",
    "B 03000000222222221111111100b40008fe5d8cb4",
    "A 03000000111111112222222201b40008b76ab3e0",
    "A 78000008510182e8ccef",
    "B 780001084d7b2270726f746f223a312c227374617465223a2272756e6e696e67227d54171af3",
    "A 700101084445001000000b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe23486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799ec3e80d32577ca1c6eb10355a7fa4c9ee13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db00254a6f94b9de03284d7297bce1062b50759abfe4092e53789dc2e70c68b2c471",
    "A 6002010831567ba0c5ea0f34597ea3c8ed12375c81a6cbf0153a5f84a9cef3183d6287acd1f61b40658aafd4f91e43688db2d7fc21466b90b5daff24496e93b8dd02274c7196bbe0052a4f7499bee3082d52779cc1e60b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe23486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799ec3e80d32577ca1c6eb10bbd8489a",
    "A 60030108355a7fa4c9ee13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db00254a6f94b9de03284d7297bce1062b50759abfe4092e53789dc2e70c31567ba0c5ea0f34597ea3c8ed12375c81a6cbf0153a5f84a9cef3183d6287acd1f61b40658aafd4f91e43688db2d7fc21466b90b5daff24496e93b8dd02274c7196bbe0052a4f7499bee3082d52779cc1e60b30557a9fc4e90e33587da2c7ec11365b80a5caef14e4ad91c4",
    "A 68040108395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe23486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799e56848de6",
    "B 020005081fa34050",
    "A 02000108c3c2ca1e",
    "B 020005081fa34050",
];

fn check(got: &[String], want: &[&str]) {
    if got != want {
        let mut printed = String::new();
        for line in got {
            printed.push_str(&format!("    \"{line}\",\n"));
        }
        panic!("the update channel's bytes moved; this run produced:\n{printed}");
    }
}

type UsbLink = Link<SelectiveRepeat>;

/// The exchange, each frame as `"<A|B> <hex>"`. A is the host, B the board.
fn transcript(cfg: LinkConfig) -> Vec<String> {
    let mut a = UsbLink::new(cfg.clone(), 0x1111_1111);
    let mut b = UsbLink::new(cfg.clone(), 0x2222_2222);
    let mut out = Vec::new();
    let mut now: Micros = 0;
    let framing = cfg.framing;
    let step = |now: Micros, a: &mut UsbLink, b: &mut UsbLink, out: &mut Vec<String>| {
        for _ in 0..32 {
            let mut moved = false;
            if let Some(f) = a.poll_transmit(now) {
                let f = f.to_vec();
                out.push(format!("A {}", hex(&f)));
                feed(b, framing, now, &f);
                moved = true;
            }
            if let Some(f) = b.poll_transmit(now) {
                let f = f.to_vec();
                out.push(format!("B {}", hex(&f)));
                feed(a, framing, now, &f);
                moved = true;
            }
            if !moved {
                break;
            }
        }
        while a.recv().is_some() {}
        while b.recv().is_some() {}
    };
    step(now, &mut a, &mut b, &mut out);
    assert_eq!(a.state(), LinkState::Established);
    assert_eq!(b.state(), LinkState::Established);
    // `Q proto=1`, and an answer shaped like `M` (a short JSON).
    a.send(CH_UPDATE, b"Q\x01").unwrap();
    b.send(CH_UPDATE, b"M{\"proto\":1,\"state\":\"running\"}")
        .unwrap();
    step(now, &mut a, &mut b, &mut out);
    // A chunk cut into fragments (three on usb's 256-byte payload): `D E`
    // at offset 0x1000, 600 bytes.
    let mut chunk = vec![b'D', b'E', 0x00, 0x10, 0x00, 0x00];
    chunk.extend((0..600u32).map(|i| (i * 37 + 11) as u8));
    a.send(CH_UPDATE, &chunk).unwrap();
    step(now, &mut a, &mut b, &mut out);
    for dt in [1_000, 5_000, 300_000, 1_100_000] {
        now += dt;
        step(now, &mut a, &mut b, &mut out);
    }
    out
}

fn feed(link: &mut UsbLink, framing: Framing, now: Micros, f: &[u8]) {
    match framing {
        Framing::Stream => link.on_bytes(now, f),
        Framing::Datagram => link.on_datagram(now, f),
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
