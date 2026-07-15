use std::sync::Arc;
use std::time::{Duration, Instant};

use lpa_client::stream::DeviceByteStream;
use lpa_client::transport_serial::create_hardware_serial_transport_pair_with_options;

use crate::provider::endpoint::LinkEndpointId;
use crate::providers::fake::FakeProvider;
use crate::{LinkManagementRequest, LinkManagementResult, LinkProvider};

use super::*;

#[test]
fn blank_flash_repeats_the_invalid_header_line() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::BlankFlash));
    let mut stream = FakeDeviceByteStream::new(device);

    let lines = read_lines_until(&mut stream, Duration::from_millis(500), |lines| {
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

    let lines = read_lines_until(&mut stream, Duration::from_millis(300), |lines| {
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

    let lines = read_lines_until(&mut stream, Duration::from_millis(300), |lines| {
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

    let lines = read_lines_until(&mut stream, Duration::from_millis(300), |lines| {
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

    let first = read_lines_until(&mut stream, Duration::from_millis(300), |lines| {
        !lines.is_empty()
    });
    assert!(!first.is_empty());

    // The hardware transport's reset-after-open dance (RTS pulse, DTR low).
    stream.set_signals(Some(false), None).unwrap();
    stream.set_signals(None, Some(true)).unwrap();
    stream.set_signals(Some(false), None).unwrap();
    stream.set_signals(None, Some(true)).unwrap();
    stream.set_signals(None, Some(false)).unwrap();

    let replay = read_lines_until(&mut stream, Duration::from_millis(300), |lines| {
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
    let identity = FakeDeviceIdentity::new("dev_fakefakefakefak0", "Bench fake");
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_project_files(vec![(
                "project.json".to_string(),
                br#"{"kind":"Project","uid":"prj_fakefakefakefak0","name":"Fake","nodes":{}}"#
                    .to_vec(),
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

    // The explicit hello round-trips through the real M! framing; the
    // unsolicited boot hello is also observed by the client wrapper.
    let hello = client.hello().await.unwrap();
    assert_eq!(hello.proto, lpc_wire::WIRE_PROTO_VERSION);
    assert_eq!(hello.fw.package, "fw-esp32");
    assert_eq!(hello.device_uid.as_deref(), Some("dev_fakefakefakefak0"));

    let projects = client.project_list_available().await.unwrap();
    assert!(
        projects
            .iter()
            .any(|project| project.path.as_str().contains("studio")),
        "seeded project storage is visible over the wire: {projects:?}"
    );
}

#[test]
fn premature_writes_during_boot_are_discarded_and_counted() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new().with_boot_delay(Duration::from_millis(300)),
    )));
    let mut stream = FakeDeviceByteStream::new(device.clone());

    stream
        .write_all(b"M!{\"id\":1,\"msg\":\"Hello\"}\n")
        .unwrap();

    assert!(
        device.premature_input_bytes() > 0,
        "bytes written before the server loop runs are dropped, like real hardware"
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
    // Cut the very first protocol frame (the unsolicited hello) in half.
    device.set_failure_plan(FakeFailurePlan::none().with_cut_mid_frame_after_frames(0));
    let mut stream = FakeDeviceByteStream::new(device);

    let mut collected = Vec::new();
    let deadline = Instant::now() + Duration::from_millis(700);
    while Instant::now() < deadline {
        let mut buf = [0u8; 256];
        match stream.read_available(&mut buf) {
            Ok(n) => collected.extend_from_slice(&buf[..n]),
            Err(error) => panic!("mid-frame cut must stall, not error: {error}"),
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let text = String::from_utf8_lossy(&collected);
    let frame_start = text.find("M!").expect("a frame starts");
    assert!(
        !text[frame_start..].contains('\n'),
        "the cut frame never completes its line: {text:?}"
    );
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

    let lines = read_lines_until(&mut stream, Duration::from_millis(700), |lines| {
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
    let mut provider = FakeProvider::new().with_device_endpoint(
        endpoint_id.clone(),
        "Fake ESP32",
        FakeDeviceScript::new(FakeBootState::BlankFlash),
    );
    let session = provider.connect(&endpoint_id).await.unwrap();
    let device = provider.device(&endpoint_id).unwrap();

    let flashed = provider
        .manage(session.id(), LinkManagementRequest::FlashFirmware)
        .await
        .unwrap();
    assert!(matches!(flashed, LinkManagementResult::FlashFirmware(_)));

    // Flashed device boots as LightPlayer: its stream announces the M2
    // server-start line.
    let mut stream = FakeDeviceByteStream::new(device.clone());
    let lines = read_lines_until(&mut stream, Duration::from_millis(700), |lines| {
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
    let lines = read_lines_until(&mut stream, Duration::from_millis(500), |lines| {
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
    let mut provider = FakeProvider::new().with_device_endpoint(
        endpoint_id.clone(),
        "Fake ESP32",
        FakeDeviceScript::new(FakeBootState::BlankFlash)
            .with_manage_failure("bootloader sync failed"),
    );
    let session = provider.connect(&endpoint_id).await.unwrap();

    let failed = provider
        .manage(session.id(), LinkManagementRequest::FlashFirmware)
        .await;
    assert!(matches!(failed, Err(crate::LinkError::Other { .. })));

    // The failure is one-shot: the retry succeeds.
    let retried = provider
        .manage(session.id(), LinkManagementRequest::FlashFirmware)
        .await;
    assert!(retried.is_ok());

    provider.close(session.id()).await.unwrap();
}

/// Poll the stream, splitting output into lines, until `done` or timeout.
fn read_lines_until(
    stream: &mut FakeDeviceByteStream,
    timeout: Duration,
    done: impl Fn(&[String]) -> bool,
) -> Vec<String> {
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::new();
    loop {
        let mut buf = [0u8; 256];
        if let Ok(n) = stream.read_available(&mut buf) {
            bytes.extend_from_slice(&buf[..n]);
        }
        let lines: Vec<String> = String::from_utf8_lossy(&bytes)
            .lines()
            .map(str::to_string)
            .collect();
        if done(&lines) || Instant::now() >= deadline {
            return lines;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
