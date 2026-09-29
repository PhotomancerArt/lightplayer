use std::sync::Arc;
use std::time::{Duration, Instant};

use lpa_client::stream::DeviceByteStream;
use lpa_client::transport_serial::create_hardware_serial_transport_pair_with_options;

use lpc_wire::ClientMessage;

use crate::provider::endpoint::LinkEndpointId;
use crate::providers::fake::FakeProvider;
use crate::{LinkManagementRequest, LinkManagementResult, LinkProvider};

use super::*;

#[test]
fn blank_flash_repeats_the_invalid_header_line() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::BlankFlash));
    let mut stream = FakeDeviceByteStream::new(device);

    let lines =
        HostPeer::new().read_lines_until(&mut stream, Duration::from_millis(500), |lines| {
            lines
                .iter()
                .filter(|line| line.contains("invalid header: 0xffffffff"))
                .count()
                >= 2
        });

    assert!(
        lines
            .iter()
            .filter(|line| line.contains("invalid header: 0xffffffff"))
            .count()
            >= 2,
        "blank flash repeats the ROM's invalid-header line: {lines:?}"
    );
}

#[test]
fn rom_download_mode_announces_waiting_for_download_once() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::RomDownloadMode));
    let mut stream = FakeDeviceByteStream::new(device);

    let lines =
        HostPeer::new().read_lines_until(&mut stream, Duration::from_millis(300), |lines| {
            lines
                .iter()
                .any(|line| line.contains("waiting for download"))
        });

    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains("waiting for download"))
            .count(),
        1
    );
}

#[test]
fn foreign_firmware_announces_its_known_boot_string() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::ForeignFirmware));
    let mut stream = FakeDeviceByteStream::new(device);

    let lines =
        HostPeer::new().read_lines_until(&mut stream, Duration::from_millis(300), |lines| {
            !lines.is_empty()
        });

    assert!(
        lines
            .iter()
            .any(|line| line.contains("Hello from Seeed Studio XIAO ESP32-C6")),
        "foreign firmware boot string missing: {lines:?}"
    );
}

#[test]
fn usb_jtag_download_sequence_drops_into_rom_download_mode() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::ForeignFirmware));
    let mut stream = FakeDeviceByteStream::new(device);

    // The browser controller's usb-jtag-download dance:
    // R0 D0 W100 D1 R0 W100 R1 D0 R1 W100 R0 D0 (waits elided — the fake
    // keys on edges, not timing).
    for (dtr, rts) in [
        (None, Some(false)),
        (Some(false), None),
        (Some(true), None),
        (None, Some(false)),
        (None, Some(true)),
        (Some(false), None),
        (None, Some(true)),
        (None, Some(false)),
        (Some(false), None),
    ] {
        stream.set_signals(dtr, rts).unwrap();
    }

    let lines =
        HostPeer::new().read_lines_until(&mut stream, Duration::from_millis(300), |lines| {
            lines
                .iter()
                .any(|line| line.contains("waiting for download"))
        });
    assert!(
        lines
            .iter()
            .any(|line| line.contains("waiting for download")),
        "download dance should land in ROM download mode: {lines:?}"
    );
}

#[test]
fn hard_reset_replays_the_current_boot() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::ForeignFirmware));
    let mut stream = FakeDeviceByteStream::new(device);

    let first =
        HostPeer::new().read_lines_until(&mut stream, Duration::from_millis(300), |lines| {
            !lines.is_empty()
        });
    assert!(!first.is_empty());

    // The hardware transport's reset-after-open dance (RTS pulse, DTR low).
    stream.set_signals(Some(false), None).unwrap();
    stream.set_signals(None, Some(true)).unwrap();
    stream.set_signals(Some(false), None).unwrap();
    stream.set_signals(None, Some(true)).unwrap();
    stream.set_signals(None, Some(false)).unwrap();

    let replay =
        HostPeer::new().read_lines_until(&mut stream, Duration::from_millis(300), |lines| {
            !lines.is_empty()
        });
    assert!(
        replay
            .iter()
            .any(|line| line.contains("Hello from Seeed Studio XIAO ESP32-C6")),
        "hard reset replays the same state's boot: {replay:?}"
    );
}

#[tokio::test]
async fn light_player_state_speaks_real_frames_through_the_real_transport() {
    let identity = FakeDeviceIdentity::new("devfakefakefakefake", "Bench fake");
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(vec![(
                "project.json".to_string(),
                br#"{"format":7,"uid":"prjfakefakefakefake","name":"Fake"}"#.to_vec(),
            )])
            .with_identity(identity),
    )));
    let stream = FakeDeviceByteStream::new(device);
    let transport = create_hardware_serial_transport_pair_with_options(
        Box::new(stream),
        "fake-device-test",
        Default::default(),
    )
    .unwrap();
    let transport: Box<dyn lpa_client::ClientTransport> = Box::new(transport);
    let client =
        lpa_client::TokioLpClient::new_shared(Arc::new(tokio::sync::Mutex::new(transport)));

    // The explicit hello round-trips through the real link; the unsolicited
    // boot hello is also observed by the client wrapper.
    let hello = client.hello().await.unwrap();
    assert_eq!(hello.proto, lpc_wire::WIRE_PROTO_VERSION);
    assert_eq!(hello.build.package, "fw-esp32c6");
    assert_eq!(hello.device_uid.as_deref(), Some("devfakefakefakefake"));

    let projects = client.project_list_available().await.unwrap();
    assert!(
        projects
            .iter()
            .any(|project| project.path.as_str().contains("studio")),
        "seeded project storage is visible over the wire: {projects:?}"
    );
}

#[tokio::test]
async fn hello_reflects_a_runtime_root_identity_stamp() {
    use lpc_model::AsLpPath;

    // An unstamped device: no identity file at the fs root.
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new(),
    )));
    let stream = FakeDeviceByteStream::new(device);
    let transport = create_hardware_serial_transport_pair_with_options(
        Box::new(stream),
        "fake-device-stamp-test",
        Default::default(),
    )
    .unwrap();
    let transport: Box<dyn lpa_client::ClientTransport> = Box::new(transport);
    let client =
        lpa_client::TokioLpClient::new_shared(Arc::new(tokio::sync::Mutex::new(transport)));

    let hello = client.hello().await.unwrap();
    assert_eq!(hello.device_uid, None, "unstamped device carries no uid");

    // Stamp over the wire (the studio's flow: a root FsRequest::Write),
    // then ask again: the server re-reads the root file per request, so
    // the new uid arrives without a reboot.
    client
        .fs_write(
            fw_host::DEVICE_IDENTITY_PATH.as_path(),
            br#"{"uid":"devstampstampstamp","name":"Freshly named"}"#.to_vec(),
        )
        .await
        .unwrap();
    let hello = client.hello().await.unwrap();
    assert_eq!(hello.device_uid.as_deref(), Some("devstampstampstamp"));
}

/// R4a end to end through the real adapter: the device stamps identity on
/// every heartbeat, so a client that attached mid-stream — long after the
/// boot hello it never saw — learns who this board is from the unsolicited
/// channel alone, within one heartbeat period.
#[test]
fn a_heartbeat_carries_identity_so_a_mid_stream_attach_resolves_without_a_hello() {
    use lpa_devices::identity::{DeviceUid, MacAddress};

    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new("devfakefakefakefake", "Bench fake"))
            .with_base_mac(FAKE_PROBED_MAC)
            .with_heartbeat_interval(Duration::from_millis(20)),
    )));
    let mut stream = FakeDeviceByteStream::new(device);

    let lines =
        HostPeer::new().read_lines_until(&mut stream, Duration::from_millis(700), |lines| {
            lines.iter().any(|line| heartbeat_frame(line).is_some())
        });

    let identity = lines
        .iter()
        .find_map(|line| heartbeat_frame(line))
        .and_then(|frame| frame.identity().cloned())
        .unwrap_or_else(|| panic!("a heartbeat announces who the device is: {lines:?}"));
    assert_eq!(
        identity.uid,
        Some(DeviceUid("devfakefakefakefake".to_string()))
    );
    // The script reports the MAC in the uppercase spelling a device is
    // allowed to use; the adapter normalizes, or the same board would bind
    // twice.
    assert_eq!(
        identity.mac,
        Some(MacAddress("60:55:f9:0a:0b:0c".to_string()))
    );
}

/// R4b: `ClientRequest::Reboot` is ANSWERED and then honored. The ack must
/// reach the wire before the board goes down — a reset that eats its own
/// answer leaves the recovery ladder waiting on a response that no longer
/// has a sender.
#[test]
fn a_reboot_request_is_answered_before_the_device_resets() {
    const BANNER: &str = "[INIT] LightPlayer fake device booting";

    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new(),
    )));
    let mut stream = FakeDeviceByteStream::new(device.clone());
    let mut peer = HostPeer::new();

    // Let the first boot finish and the link come up, so the reboot's
    // banner is unambiguous.
    peer.read_lines_until(&mut stream, Duration::from_millis(500), |lines| {
        lines.iter().any(|line| line.contains("\"hello\""))
    });
    peer.send(ClientMessage {
        id: 7,
        msg: lpc_wire::ClientRequest::Reboot,
    });

    // Each read window starts a fresh line list, so these lines are the
    // ones that arrived AFTER the request: the ack, then the second boot.
    let lines = peer.read_lines_until(&mut stream, Duration::from_millis(1500), |lines| {
        lines.iter().any(|line| line.contains(BANNER))
    });

    assert_eq!(
        device.reboot_requests(),
        1,
        "the embedder reset hook observed exactly one request: {lines:?}"
    );
    let ack = lines
        .iter()
        .position(|line| line.starts_with("M!") && line.contains("\"id\":7"))
        .unwrap_or_else(|| panic!("the reboot request is answered on the wire: {lines:?}"));
    let reboot = lines
        .iter()
        .position(|line| line.contains(BANNER))
        .unwrap_or_else(|| panic!("the device reboots: {lines:?}"));
    assert!(
        ack < reboot,
        "the ack reaches the wire before the reset it causes: {lines:?}"
    );
}

/// Decode one COMPLETE `M!` heartbeat line through the shipped adapter.
///
/// Decoding is also the completeness test: a read window can end mid-frame,
/// and a truncated line is not evidence of anything.
fn heartbeat_frame(line: &str) -> Option<lpa_devices::wire::ServerFrame> {
    use lpa_devices::wire::ServerFrameBody;

    let json = line.strip_prefix("M!")?;
    let frame = crate::device_link::wire::decode_server_frame(json).ok()?;
    matches!(frame.body, ServerFrameBody::Heartbeat { .. }).then_some(frame)
}

#[test]
fn premature_writes_during_boot_are_discarded_and_counted() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new().with_boot_delay(Duration::from_millis(300)),
    )));
    let mut stream = FakeDeviceByteStream::new(device.clone());

    // A request from a host whose session was with the board's previous
    // boot: a data frame arriving while nothing serves.
    stream
        .write_all(&stale_session_request_frame(r#"{"id":1,"msg":"hello"}"#))
        .unwrap();

    assert!(
        device.premature_input_bytes() > 0,
        "requests written before the server loop runs are dropped, like real hardware"
    );
    assert_eq!(device.premature_input(), "M!{\"id\":1,\"msg\":\"hello\"}\n");
}

/// A host handshaking with a board that is not serving is not talking
/// before readiness: its link frames are not requests.
#[test]
fn a_host_handshake_before_boot_is_not_premature_input() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::BlankFlash));
    let mut stream = FakeDeviceByteStream::new(device.clone());

    HostPeer::new().read_lines_until(&mut stream, Duration::from_millis(300), |_| false);

    assert_eq!(device.premature_input_bytes(), 0);
}

/// A byte lost on the way to the host is resent by the link: the host reads
/// the hello anyway, and never a damaged message.
#[test]
fn a_dropped_byte_is_resent_under_the_messages() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new(),
    )));
    let mut stream = FakeDeviceByteStream::new(device.clone());
    let mut peer = HostPeer::new();
    // Let the boot text through, then lose one byte inside the link's frames.
    peer.read_lines_until(&mut stream, Duration::from_millis(300), |lines| {
        lines
            .iter()
            .any(|line| line.contains("starting server loop"))
    });
    device.set_failure_plan(FakeFailurePlan::none().with_drop_byte_at(device.served_bytes() + 40));

    let lines = peer.read_lines_until(&mut stream, Duration::from_millis(2000), |lines| {
        lines.iter().any(|line| line.contains("\"hello\""))
    });

    assert!(
        lines.iter().any(|line| line.contains("\"hello\"")),
        "the hello arrives whole: {lines:?}"
    );
    assert!(
        peer.port.counters().damaged + peer.port.counters().stale_partials > 0,
        "the lost byte damaged a frame the link then resent"
    );
}

#[test]
fn disconnect_knob_surfaces_as_closed_stream() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::BlankFlash));
    device.set_failure_plan(FakeFailurePlan::none().with_disconnect_after_bytes(5));
    let mut stream = FakeDeviceByteStream::new(device);

    let mut served = 0;
    let deadline = Instant::now() + Duration::from_millis(500);
    let error = loop {
        let mut buf = [0u8; 64];
        match stream.read_available(&mut buf) {
            Ok(n) => served += n,
            Err(error) => break error,
        }
        assert!(Instant::now() < deadline, "disconnect knob never fired");
        std::thread::sleep(Duration::from_millis(5));
    };

    assert_eq!(error, lpa_client::ByteStreamError::Closed);
    assert!(served <= 5, "no bytes beyond the disconnect threshold");
}

#[test]
fn stall_knob_stops_responding_without_eof() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::BlankFlash));
    device.set_failure_plan(FakeFailurePlan::none().with_stall_after_bytes(5));
    let mut stream = FakeDeviceByteStream::new(device);

    let mut served = 0;
    let deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < deadline {
        let mut buf = [0u8; 64];
        served += stream.read_available(&mut buf).unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }

    assert_eq!(served, 5, "exactly the pre-stall bytes are served, no EOF");
}

#[tokio::test]
async fn mid_frame_cut_truncates_a_frame_then_stalls() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new(),
    )));
    // Cut the frame carrying the very first reply (the hello) in half.
    device.set_failure_plan(FakeFailurePlan::none().with_cut_mid_frame_after_frames(0));
    let mut stream = FakeDeviceByteStream::new(device.clone());
    let mut peer = HostPeer::new();

    let lines = peer.read_lines_until(&mut stream, Duration::from_millis(700), |_| false);

    assert!(
        lines
            .iter()
            .any(|line| line.contains("starting server loop")),
        "the board booted: {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("M!")),
        "the cut reply never completes, and the board says nothing after it: {lines:?}"
    );
    assert!(peer.saw_up, "the link came up before the board hung");
}

#[tokio::test]
async fn log_flood_interleaves_device_lines_between_frames() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new(),
    )));
    device.set_failure_plan(
        FakeFailurePlan::none().with_log_flood_line("[FLOOD] chatty firmware log"),
    );
    let mut stream = FakeDeviceByteStream::new(device);

    let lines =
        HostPeer::new().read_lines_until(&mut stream, Duration::from_millis(700), |lines| {
            lines.iter().any(|line| line.starts_with("M!"))
        });

    let frame_index = lines
        .iter()
        .position(|line| line.starts_with("M!"))
        .expect("a protocol frame arrives");
    assert!(
        lines[..frame_index]
            .iter()
            .any(|line| line.contains("[FLOOD]")),
        "the flood line precedes the frame on the shared wire: {lines:?}"
    );
}

#[tokio::test]
async fn provider_manage_runs_scripted_flash_and_erase_transitions() {
    let endpoint_id = LinkEndpointId::new("fake-device-0");
    let provider = FakeProvider::new().with_device_endpoint(
        endpoint_id.clone(),
        "Fake ESP32",
        FakeDeviceScript::new(FakeBootState::BlankFlash),
    );
    let session = provider.connect(&endpoint_id).await.unwrap();
    let device = provider.device(&endpoint_id).unwrap();

    let flashed = provider
        .manage(
            session.id(),
            LinkManagementRequest::FlashFirmware { build_id: None },
        )
        .await
        .unwrap();
    assert!(matches!(flashed, LinkManagementResult::FlashFirmware(_)));

    // Flashed device boots as LightPlayer: its stream announces the M2
    // server-start line.
    let mut stream = FakeDeviceByteStream::new(device.clone());
    let lines =
        HostPeer::new().read_lines_until(&mut stream, Duration::from_millis(700), |lines| {
            lines
                .iter()
                .any(|line| line.contains("fw-esp32 initialized, starting server loop"))
        });
    assert!(
        lines.iter().any(|line| line.contains(&format!(
            "proto={} commit={FAKE_IMAGE_IDENTITY}",
            lpc_wire::WIRE_PROTO_VERSION
        ))),
        "the boot line carries the flashed image identity: {lines:?}"
    );

    let erased = provider
        .manage(session.id(), LinkManagementRequest::EraseDeviceFlash)
        .await
        .unwrap();
    assert!(matches!(erased, LinkManagementResult::EraseDeviceFlash(_)));
    let lines =
        HostPeer::new().read_lines_until(&mut stream, Duration::from_millis(500), |lines| {
            lines
                .iter()
                .any(|line| line.contains("invalid header: 0xffffffff"))
        });
    assert!(
        lines
            .iter()
            .any(|line| line.contains("invalid header: 0xffffffff")),
        "erase lands back on blank flash: {lines:?}"
    );

    provider.close(session.id()).await.unwrap();
}

#[tokio::test]
async fn scripted_manage_failure_fails_the_next_operation_once() {
    let endpoint_id = LinkEndpointId::new("fake-device-0");
    let provider = FakeProvider::new().with_device_endpoint(
        endpoint_id.clone(),
        "Fake ESP32",
        FakeDeviceScript::new(FakeBootState::BlankFlash)
            .with_manage_failure("bootloader sync failed"),
    );
    let session = provider.connect(&endpoint_id).await.unwrap();

    let failed = provider
        .manage(
            session.id(),
            LinkManagementRequest::FlashFirmware { build_id: None },
        )
        .await;
    assert!(matches!(failed, Err(crate::LinkError::Other { .. })));

    // The failure is one-shot: the retry succeeds.
    let retried = provider
        .manage(
            session.id(),
            LinkManagementRequest::FlashFirmware { build_id: None },
        )
        .await;
    assert!(retried.is_ok());

    provider.close(session.id()).await.unwrap();
}

/// A host's end of the fake board's link, driven by hand: what it reads
/// comes out as lines, console text as it is and each wire message as its
/// `M!{json}` line.
struct HostPeer {
    port: lpc_wire::WireLinkPort,
    started: Instant,
    saw_up: bool,
}

impl HostPeer {
    fn new() -> Self {
        Self {
            port: lpc_wire::WireLinkPort::new(
                lpa_client::transport_serial::fresh_link_nonce(),
                false,
            ),
            started: Instant::now(),
            saw_up: false,
        }
    }

    fn now(&self) -> u64 {
        self.started.elapsed().as_micros() as u64
    }

    fn send(&mut self, message: ClientMessage) {
        self.port.send_client(&message).unwrap();
    }

    /// Run the link over `stream`, collecting lines, until `done` or timeout.
    fn read_lines_until(
        &mut self,
        stream: &mut FakeDeviceByteStream,
        timeout: Duration,
        done: impl Fn(&[String]) -> bool,
    ) -> Vec<String> {
        let deadline = Instant::now() + timeout;
        let mut lines = Vec::new();
        loop {
            let now = self.now();
            while let Some(frame) = self.port.poll_transmit(now) {
                stream.write_all(frame).unwrap();
            }
            let mut buf = [0u8; 256];
            if let Ok(n) = stream.read_available(&mut buf) {
                let now = self.now();
                self.port.on_bytes(now, &buf[..n]);
            }
            while let Some(read) = self.port.poll_read() {
                match read {
                    lpc_wire::PortRead::Message(payload) => {
                        lines.push(format!("M!{}", payload.json))
                    }
                    lpc_wire::PortRead::Log(line) => lines.push(line),
                    lpc_wire::PortRead::Up { .. } => self.saw_up = true,
                    _ => {}
                }
            }
            if done(&lines) || Instant::now() >= deadline {
                return lines;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

/// The bytes of a data frame carrying `json` from a host whose session was
/// with some other board end (the board's previous boot).
fn stale_session_request_frame(json: &str) -> Vec<u8> {
    use lpc_wire::lp_link::{CH_PROTO, Link, LinkConfig, SelectiveRepeat};
    let mut host: Link<SelectiveRepeat> = Link::new(LinkConfig::usb(), 0x1111_0001);
    let mut board: Link<SelectiveRepeat> = Link::new(LinkConfig::usb(), 0x2222_0001);
    for step in 0..20u64 {
        let now = step * 1_000;
        while let Some(frame) = host.poll_transmit(now) {
            let frame = frame.to_vec();
            board.on_bytes(now, &frame);
        }
        while let Some(frame) = board.poll_transmit(now) {
            let frame = frame.to_vec();
            host.on_bytes(now, &frame);
        }
        while host.recv().is_some() {}
        while board.recv().is_some() {}
    }
    assert_eq!(host.state(), lpc_wire::lp_link::LinkState::Established);
    host.send(CH_PROTO, json.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    while let Some(frame) = host.poll_transmit(30_000) {
        bytes.extend_from_slice(frame);
    }
    bytes
}
