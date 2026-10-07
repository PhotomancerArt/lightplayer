//! **An old core must take a new one.** This host's pieces, driven by the
//! board side of `lpc-update`'s v1 golden transcript
//! (`lp-core/lpc-update/tests/v1_golden.hex`): every answer must decode as
//! the golden's host messages do. When a later protocol version adds
//! fields, this keeps proving that the current host still serves a v1
//! board.

use lpa_update::{
    Credential, DriverConfig, DriverEffect, Finish, HostBuild, HostIdentity, HostRefusal,
    LoginClient, LoginEvent, ServeConfig, ServeEvent, ServeSession, StopReason, UpdateDriver,
};
use lpc_update::{BoardManifest, BoardMessage, HostLoginStep, HostMessage, PieceKind};

/// The golden's lines: `(board to host?, bytes)`.
fn golden() -> Vec<(bool, Vec<u8>)> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../lp-core/lpc-update/tests/v1_golden.hex"
    );
    let text = std::fs::read_to_string(path).expect("the v1 golden");
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let mut parts = l.split_whitespace();
            let dir = parts.next().unwrap();
            let ty = parts.next().unwrap().as_bytes()[0];
            let hex = parts.next().unwrap_or("");
            let mut bytes = vec![ty];
            bytes.extend(
                (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()),
            );
            (dir == "B>H", bytes)
        })
        .collect()
}

fn board_lines(ty: u8) -> Vec<Vec<u8>> {
    golden()
        .into_iter()
        .filter(|(from_board, b)| *from_board && b[0] == ty)
        .map(|(_, b)| b)
        .collect()
}

fn host_line(ty: u8) -> Vec<u8> {
    golden()
        .into_iter()
        .find(|(from_board, b)| !*from_board && b[0] == ty)
        .map(|(_, b)| b)
        .unwrap()
}

fn host_build() -> HostBuild {
    HostBuild::from_parts(
        HostIdentity {
            target: "esp32c6-4mb".into(),
            chip: "esp32c6".into(),
            version: "2026.10.06-1".into(),
            build_id: "2026.10.06-1+def456789012".into(),
            wire_proto: 36,
            layout: 1,
            min_loader: 1,
        },
        vec![1; 3 * 4096],
        vec![2; 5 * 4096],
        None,
        None,
    )
    .unwrap()
}

fn sends(effects: &[DriverEffect]) -> Vec<Vec<u8>> {
    effects
        .iter()
        .filter_map(|e| match e {
            DriverEffect::Send(b) => Some(b.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_driver_asks_and_backs_up_in_the_goldens_own_bytes() {
    // One request at a time: the golden holds one `G`.
    let config = DriverConfig {
        serve: ServeConfig { ahead: 1 },
        ..DriverConfig::default()
    };
    let mut d = UpdateDriver::new(host_build(), config);
    d.go();
    d.link_up(0);
    assert_eq!(
        sends(&d.take_effects()),
        [host_line(b'Q')],
        "Q is the golden's"
    );
    // The golden's board: running an older release whose engine no cache or
    // store holds, so the backup reads it back.
    let m = &board_lines(b'M')[0];
    assert!(BoardManifest::from_json(&m[1..]).is_ok());
    d.on_board(1, m, &[]);
    let effects = d.take_effects();
    assert!(effects.iter().any(|e| matches!(
        e,
        DriverEffect::Decided(lpa_update::Decision::OfferUpdate { .. })
    )));
    d.source_result(lpa_update::decide::SourceResult::Cache(None));
    d.take_effects();
    d.source_result(lpa_update::decide::SourceResult::Store(
        lpa_update::decide::StoreAnswer::NotFound,
    ));
    assert_eq!(
        sends(&d.take_effects()),
        [host_line(b'G')],
        "G is the golden's"
    );
}

#[test]
fn requests_are_served_in_the_goldens_shapes() {
    let b = host_build();
    // One chunk per request: each golden `R` answered by its own chunk.
    let mut s = ServeSession::new(ServeConfig { ahead: 1 });
    for r in board_lines(b'R') {
        let out = s.on_board(&b, &r);
        let Ok(BoardMessage::Request(req)) = BoardMessage::decode(&r) else {
            panic!()
        };
        let Ok(HostMessage::Chunk(c)) = HostMessage::decode(&out.send[0]) else {
            panic!("not a chunk");
        };
        assert_eq!((c.kind, c.off), (req.kind, req.off));
        assert_eq!(c.kind, PieceKind::Core);
    }
}

#[test]
fn every_golden_refusal_is_understood_and_u_means_the_board_lacks_a_message() {
    let b = host_build();
    let mut s = ServeSession::new(ServeConfig::USB);
    let events: Vec<_> = board_lines(b'N')
        .iter()
        .flat_map(|n| s.on_board(&b, n).events)
        .collect();
    assert_eq!(events.len(), 9);
    assert!(
        events
            .iter()
            .all(|e| !matches!(e, ServeEvent::Refused(HostRefusal::Other(_))))
    );
    assert_eq!(
        events.last(),
        Some(&ServeEvent::Refused(HostRefusal::BoardLacksMessage(b'X')))
    );

    // The driver, mid-offer, stops on it — "this board lacks that message",
    // never a failed update to retry.
    let build = host_build();
    let mut d = UpdateDriver::new(build.clone(), DriverConfig::default());
    d.link_up(0);
    d.take_effects();
    let mut m = BoardManifest::from_json(&board_lines(b'M')[0][1..]).unwrap();
    m.state = lpc_update::BoardState::NeedsEngine;
    m.core_sha256 = lpc_update::sha256_to_hex(&build.core.sha256);
    m.engine_sha256 = lpc_update::sha256_to_hex(&build.engine.sha256);
    let mut bytes = vec![b'M'];
    bytes.extend(m.to_json());
    d.on_board(1, &bytes, &[]);
    assert!(matches!(
        HostMessage::decode(&sends(&d.take_effects())[0]),
        Ok(HostMessage::Offer(_))
    ));
    d.on_board(2, board_lines(b'N').last().unwrap(), &[]);
    assert!(
        d.take_effects()
            .contains(&DriverEffect::Done(Finish::Stopped(
                StopReason::BoardLacksMessage(b'X')
            )))
    );
}

#[test]
fn the_login_client_answers_the_goldens_challenge_and_reads_its_verdicts() {
    let lines = board_lines(b'L');
    let mut c = LoginClient::new();
    let creds = [Credential::Password(b"pw".to_vec())];
    let Some(LoginEvent::Send(answer)) = c.on_board(&lines[0], &creds) else {
        panic!()
    };
    // Shaped like the golden's own answer: one MAC per offer (two).
    let (
        Ok(HostMessage::Login(HostLoginStep::Answer { macs })),
        Ok(HostMessage::Login(HostLoginStep::Answer { macs: golden })),
    ) = (
        HostMessage::decode(&answer),
        HostMessage::decode(&host_line_n(b'L', 1)),
    )
    else {
        panic!()
    };
    assert_eq!(macs.len(), golden.len());
    assert_eq!(
        c.on_board(&lines[1], &creds),
        Some(LoginEvent::Granted(lpc_access::Tier::Edit))
    );
    assert_eq!(
        c.on_board(&lines[2], &creds),
        Some(LoginEvent::Refused {
            retry_after_ms: 4000,
            exhausted: true
        })
    );
    assert_eq!(
        LoginClient::new().begin(),
        host_line(b'L'),
        "begin is the golden's"
    );
}

/// The `n`th host line of type `ty`.
fn host_line_n(ty: u8, n: usize) -> Vec<u8> {
    golden()
        .into_iter()
        .filter(|(from_board, b)| !*from_board && b[0] == ty)
        .nth(n)
        .map(|(_, b)| b)
        .unwrap()
}
