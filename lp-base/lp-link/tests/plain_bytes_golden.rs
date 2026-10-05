//! A plain link's bytes, pinned. The secure channel (feature `secure`) adds
//! bits to the SYN's flags byte and an extension after it, and a sealed
//! frame layout; none of that may move one byte of a plain link. These
//! transcripts were captured from `origin/main` (`3ecb85468`) before the
//! secure channel touched `frame.rs` or `link.rs`, and are compared with the
//! feature on and off (`cargo test -p lp-link` and `--features secure`).
//!
//! Each transcript is a fixed exchange between two links with fixed nonces:
//! the handshake, a reliable message each way, a fragmented one, their ACKs,
//! a log datagram, and keepalives, every frame in the order it went, as hex. A mismatch is a
//! wire change: never re-capture it to make this pass.

use lp_link::{
    Arq, CH_LOG, CH_PROTO, Framing, Link, LinkConfig, LinkState, Micros, NoArq, SelectiveRepeat,
};

#[test]
fn usb_selective_repeat_bytes_are_unchanged() {
    check(&transcript::<SelectiveRepeat>(LinkConfig::usb()), USB);
}

#[test]
fn ws_no_arq_bytes_are_unchanged() {
    check(&transcript::<NoArq>(LinkConfig::ws()), WS);
}

const USB: &[&str] = &[
    "A 00020301010511111111010101010107010804949ced00",
    "B 0002030101092222222211111111010701084e90312300",
    "A 00020301010a11111111222222220107010807a70e7700",
    "A 0002380111087b2268656c6c6f223a317df51a783200",
    "B 000238040108fe01077e01a7f9121a00",
    "B 000241100108036c6f67206c696e65c6c72ae900",
    "A 0097300101080b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe0123486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799ec3e80d32577ca1c6eb10355a7fa4c9ee13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db54254a6f94b9de03284d7297bce1062b50759abfe4092e53789dc2e70c31567ba0c5ea0f34597ea3c8ed12375c81a6cbf0153a5f84a9cef3183d6287acd1f61b40658aafd4f91e43688db2d7fc21466b90b5dafe2024496e93b8dd02274c7196bbe0052a4f7499bee3082d52779cc1e62e7346e500",
    "A 0097200201080b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe0123486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799ec3e80d32577ca1c6eb10355a7fa4c9ee13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db54254a6f94b9de03284d7297bce1062b50759abfe4092e53789dc2e70c31567ba0c5ea0f34597ea3c8ed12375c81a6cbf0153a5f84a9cef3183d6287acd1f61b40658aafd4f91e43688db2d7fc21466b90b5dafe2024496e93b8dd02274c7196bbe0052a4f7499bee3082d52779cc1e60f0ca19b00",
    "B 0002020703082df28f3900",
    "A 0062280301080b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe0123486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799e4f7e6ce400",
    "B 000202070408683be24300",
    "A 000202070108c3c2ca1e00",
    "B 000202070408683be24300",
    "A 000202070108c3c2ca1e00",
    "B 000202070408683be24300",
];

const WS: &[&str] = &[
    "A 030000001111111100000000000004100ff2332a",
    "B 0300000022222222111111110000041045f69ee4",
    "A 030000001111111122222222010004100cc1a1b0",
    "A 380000107b2268656c6c6f223a317d971533a5",
    "B 38000110ff007e01e5935750",
    "B 41000110036c6f67206c696e65d4fa4522",
    "A 380101100b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe23486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799ec3e80d32577ca1c6eb10355a7fa4c9ee13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db00254a6f94b9de03284d7297bce1062b50759abfe4092e53789dc2e70c31567ba0c5ea0f34597ea3c8ed12375c81a6cbf0153a5f84a9cef3183d6287acd1f61b40658aafd4f91e43688db2d7fc21466b90b5daff24496e93b8dd02274c7196bbe0052a4f7499bee3082d52779cc1e60b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe23486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799ec3e80d32577ca1c6eb10355a7fa4c9ee13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db00254a6f94b9de03284d7297bce1062b50759abfe4092e53789dc2e70c31567ba0c5ea0f34597ea3c8ed12375c81a6cbf0153a5f84a9cef3183d6287acd1f61b40658aafd4f91e43688db2d7fc21466b90b5daff24496e93b8dd02274c7196bbe0052a4f7499bee3082d52779cc1e60b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe23486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799e29db230b",
    "A 02000110635d4d84",
    "B 02000210faf5aab0",
];

fn check(got: &[String], want: &[&str]) {
    if got != want {
        let mut printed = String::new();
        for line in got {
            printed.push_str(&format!("    \"{line}\",\n"));
        }
        panic!("a plain link's bytes moved; this run produced:\n{printed}");
    }
}

/// The exchange, each frame as `"<A|B> <hex>"`.
fn transcript<A: Arq>(cfg: LinkConfig) -> Vec<String> {
    let mut a = Link::<A>::new(cfg.clone(), 0x1111_1111);
    let mut b = Link::<A>::new(cfg.clone(), 0x2222_2222);
    let mut out = Vec::new();
    let mut now: Micros = 0;
    let step = |now: Micros, a: &mut Link<A>, b: &mut Link<A>, out: &mut Vec<String>| {
        for _ in 0..16 {
            let mut moved = false;
            if let Some(f) = a.poll_transmit(now) {
                let f = f.to_vec();
                out.push(format!("A {}", hex(&f)));
                feed(b, cfg.framing, now, &f);
                moved = true;
            }
            if let Some(f) = b.poll_transmit(now) {
                let f = f.to_vec();
                out.push(format!("B {}", hex(&f)));
                feed(a, cfg.framing, now, &f);
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
    a.send(CH_PROTO, b"{\"hello\":1}").unwrap();
    b.send(CH_PROTO, &[0xFF, 0x00, 0x7E, 0x01]).unwrap();
    b.send(CH_LOG, b"\x03log line").unwrap();
    step(now, &mut a, &mut b, &mut out);
    // A message cut into fragments (three on usb's 256-byte payload).
    let big: Vec<u8> = (0..600u32).map(|i| (i * 37 + 11) as u8).collect();
    a.send(CH_PROTO, &big).unwrap();
    step(now, &mut a, &mut b, &mut out);
    for dt in [1_000, 5_000, 300_000, 1_100_000] {
        now += dt;
        step(now, &mut a, &mut b, &mut out);
    }
    out
}

fn feed<A: Arq>(link: &mut Link<A>, framing: Framing, now: Micros, f: &[u8]) {
    match framing {
        Framing::Stream => link.on_bytes(now, f),
        Framing::Datagram => link.on_datagram(now, f),
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
