//! DeviceSession state-machine tests against the scripted fake ESP32.
//!
//! Test edges may block and use tokio timers; the session itself only sees
//! the injected [`DeviceTimers`] factory.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use lpc_wire::{ClientMessage, ClientRequest, WIRE_PROTO_VERSION};

use crate::provider::endpoint::LinkEndpointId;
use crate::providers::fake::FakeProvider;
use crate::providers::fake_device::{
    FakeBootState, FakeDeviceIdentity, FakeDeviceScript, FakeEsp32Device, FakeFailurePlan,
    FakeLightPlayerState,
};
use crate::{LinkConnector, LinkSessionStatus};

use super::*;

#[tokio::test]
async fn unsolicited_hello_with_matching_proto_makes_the_session_ready() {
    let (connector, endpoint_id, _device) =
        fake_device_connector(FakeDeviceScript::new(FakeBootState::LightPlayer(
            FakeLightPlayerState::new()
                .with_identity(FakeDeviceIdentity::new("dev_fakefakefakefak0", "Bench")),
        )));
    let (sink, events) = recording_sink();
    let session = DeviceSession::connect(connector, &endpoint_id, test_timers(), sink)
        .await
        .unwrap();

    let state = session.wait_ready().await;

    let DeviceState::Ready { hello } = state else {
        panic!("expected Ready, got {state:?}");
    };
    assert_eq!(hello.proto, WIRE_PROTO_VERSION);
    assert_eq!(hello.device_uid.as_deref(), Some("dev_fakefakefakefak0"));
    assert_eq!(session.hello(), Some(hello));

    let states = recorded_states(&events);
    assert!(matches!(states.first(), Some(DeviceState::Booting)));
    assert!(matches!(states.last(), Some(DeviceState::Ready { .. })));
    // The boot banner reached the console feed as log lines.
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, DeviceEvent::LogLine { line } if line.contains("starting server loop"))),
        "boot lines should flow through the event sink"
    );
}

#[tokio::test]
async fn blank_flash_boot_is_diagnosed_as_blank_flash() {
    let state = readiness_outcome(FakeDeviceScript::new(FakeBootState::BlankFlash)).await;

    assert_eq!(state, DeviceState::BlankFlash);
}

#[tokio::test]
async fn rom_download_mode_is_diagnosed_as_bootloader() {
    let state = readiness_outcome(FakeDeviceScript::new(FakeBootState::RomDownloadMode)).await;

    assert_eq!(state, DeviceState::Bootloader);
}

#[tokio::test]
async fn foreign_firmware_is_diagnosed_as_foreign_firmware() {
    let state = readiness_outcome(FakeDeviceScript::new(FakeBootState::ForeignFirmware)).await;

    assert_eq!(state, DeviceState::ForeignFirmware);
}

#[tokio::test]
async fn silent_device_becomes_unresponsive_at_the_ready_deadline() {
    // A LightPlayer that will not produce output within the deadline.
    let (connector, endpoint_id, _device) =
        fake_device_connector(FakeDeviceScript::new(FakeBootState::LightPlayer(
            FakeLightPlayerState::new().with_boot_delay(Duration::from_secs(30)),
        )));
    let session = DeviceSession::connect(
        connector,
        &endpoint_id,
        short_ready_timers(Duration::from_millis(200)),
        DeviceEventSink::noop(),
    )
    .await
    .unwrap();

    let state = session.wait_ready().await;

    assert_eq!(
        state,
        DeviceState::Unresponsive {
            diagnosis: BootDiagnosis::NoSerialOutput,
        }
    );
    // The M1-preserved session status vocabulary is populated.
    assert!(matches!(
        session.session().status,
        LinkSessionStatus::Error { .. }
    ));
}

#[tokio::test]
async fn stream_disconnect_during_boot_marks_the_session_gone() {
    let (connector, endpoint_id, device) = fake_device_connector(FakeDeviceScript::new(
        FakeBootState::LightPlayer(FakeLightPlayerState::new()),
    ));
    device.set_failure_plan(FakeFailurePlan::none().with_disconnect_after_bytes(5));
    let session = DeviceSession::connect(
        connector,
        &endpoint_id,
        test_timers(),
        DeviceEventSink::noop(),
    )
    .await
    .unwrap();

    let state = session.wait_ready().await;

    assert_eq!(state, DeviceState::Gone);
    assert!(matches!(
        session.session().status,
        LinkSessionStatus::Error { .. }
    ));
}

#[tokio::test]
async fn suppressed_hello_is_incompatible_pre_hello_firmware() {
    let (connector, endpoint_id, _device) = fake_device_connector(FakeDeviceScript::new(
        FakeBootState::LightPlayer(FakeLightPlayerState::new().with_suppressed_hello()),
    ));
    let session = DeviceSession::connect(
        connector,
        &endpoint_id,
        short_ready_timers(Duration::from_millis(500)),
        DeviceEventSink::noop(),
    )
    .await
    .unwrap();

    let state = session.wait_ready().await;

    assert_eq!(
        state,
        DeviceState::Incompatible {
            reason: IncompatibleReason::NoHello,
        }
    );
    assert!(matches!(
        session.session().status,
        LinkSessionStatus::Error { .. }
    ));
}

#[tokio::test]
async fn wrong_proto_hello_is_incompatible() {
    let (connector, endpoint_id, _device) =
        fake_device_connector(FakeDeviceScript::new(FakeBootState::LightPlayer(
            FakeLightPlayerState::new().with_proto_override(WIRE_PROTO_VERSION + 999),
        )));
    let session = DeviceSession::connect(
        connector,
        &endpoint_id,
        test_timers(),
        DeviceEventSink::noop(),
    )
    .await
    .unwrap();

    let state = session.wait_ready().await;

    let DeviceState::Incompatible {
        reason: IncompatibleReason::ProtoMismatch { hello },
    } = state
    else {
        panic!("expected proto-mismatch Incompatible, got {state:?}");
    };
    assert_eq!(hello.proto, WIRE_PROTO_VERSION + 999);
}

#[tokio::test]
async fn channel_first_use_drives_readiness_without_wait_ready() {
    let (connector, endpoint_id, device) = fake_device_connector(FakeDeviceScript::new(
        FakeBootState::LightPlayer(FakeLightPlayerState::new()),
    ));
    let session = DeviceSession::connect(
        connector,
        &endpoint_id,
        test_timers(),
        DeviceEventSink::noop(),
    )
    .await
    .unwrap();

    // No explicit wait_ready: the channel's first request drives it.
    let mut client = lpa_client::LpClient::new(session.client_io());
    let hello = client.hello().await.unwrap().value;

    assert_eq!(hello.proto, WIRE_PROTO_VERSION);
    assert!(session.state().is_ready());
    assert_eq!(
        device.premature_input_bytes(),
        0,
        "nothing may be written before the device is ready"
    );
}

#[tokio::test]
async fn channel_refuses_a_device_without_firmware() {
    let (connector, endpoint_id, device) =
        fake_device_connector(FakeDeviceScript::new(FakeBootState::BlankFlash));
    let session = DeviceSession::connect(
        connector,
        &endpoint_id,
        test_timers(),
        DeviceEventSink::noop(),
    )
    .await
    .unwrap();

    let mut io = session.client_io();
    let error = io
        .send(ClientMessage {
            id: 1,
            msg: ClientRequest::Hello,
        })
        .await
        .unwrap_err();

    assert!(
        is_no_firmware_detected_message(&error.to_string()),
        "gate error should carry the classifiable prefix: {error}"
    );
    assert_eq!(session.state(), DeviceState::BlankFlash);
    assert_eq!(
        device.premature_input_bytes(),
        0,
        "the gate must block the write entirely"
    );
}

#[tokio::test]
async fn management_mode_invalidates_the_app_protocol_channel() {
    let (connector, endpoint_id, _device) = fake_device_connector(FakeDeviceScript::new(
        FakeBootState::LightPlayer(FakeLightPlayerState::new()),
    ));
    let session = DeviceSession::connect(
        connector,
        &endpoint_id,
        test_timers(),
        DeviceEventSink::noop(),
    )
    .await
    .unwrap();
    assert!(session.wait_ready().await.is_ready());

    let guard = session.try_begin_management().unwrap();
    assert_eq!(session.mode(), DeviceMode::Management);
    // Taking the mode twice is refused.
    assert!(session.try_begin_management().is_err());

    let mut io = session.client_io();
    let error = io
        .send(ClientMessage {
            id: 1,
            msg: ClientRequest::Hello,
        })
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("management"),
        "channel should explain the mode conflict: {error}"
    );

    drop(guard);
    assert_eq!(session.mode(), DeviceMode::AppProtocol);
    let mut client = lpa_client::LpClient::new(session.client_io());
    assert!(client.hello().await.is_ok());
}

#[tokio::test]
async fn close_marks_gone_and_closes_the_session_record() {
    let (connector, endpoint_id, _device) = fake_device_connector(FakeDeviceScript::new(
        FakeBootState::LightPlayer(FakeLightPlayerState::new()),
    ));
    let session = DeviceSession::connect(
        Rc::clone(&connector),
        &endpoint_id,
        test_timers(),
        DeviceEventSink::noop(),
    )
    .await
    .unwrap();
    assert!(session.wait_ready().await.is_ready());
    let session_record = session.session();
    let mut io = session.client_io();

    session.close().await.unwrap();

    // Channel clones observe the closed session cleanly.
    let error = io
        .send(ClientMessage {
            id: 1,
            msg: ClientRequest::Hello,
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("gone"), "{error}");
    // The provider-side record is closed (connection handoff now fails).
    let connection = crate::LinkProvider::connection(&*connector, &session_record.id).await;
    assert!(matches!(connection, Err(crate::LinkError::Closed)));
}

#[tokio::test]
async fn snapshot_carries_state_session_and_recent_lines() {
    let (connector, endpoint_id, _device) =
        fake_device_connector(FakeDeviceScript::new(FakeBootState::ForeignFirmware));
    let session = DeviceSession::connect(
        connector,
        &endpoint_id,
        test_timers(),
        DeviceEventSink::noop(),
    )
    .await
    .unwrap();
    session.wait_ready().await;

    let snapshot = session.snapshot();

    assert_eq!(snapshot.state, DeviceState::ForeignFirmware);
    assert_eq!(snapshot.session.id, session.session().id);
    assert!(
        snapshot
            .recent_lines
            .iter()
            .any(|line| line.contains("Seeed Studio")),
        "snapshot should carry the diagnostic boot tail: {:?}",
        snapshot.recent_lines
    );
}

#[tokio::test]
async fn record_level_fake_endpoint_is_rejected_as_not_hardware() {
    // A record-level endpoint (no scripted device) exposes no host protocol
    // channel; DeviceSession is hardware-only and must refuse it.
    let provider = FakeProvider::new().with_endpoint(crate::LinkEndpoint::new(
        "fake-runtime",
        crate::LinkProviderKind::Fake,
        "Fake runtime",
    ));
    let connector = Rc::new(LinkConnector::Fake(provider));

    let result = DeviceSession::connect(
        connector,
        &LinkEndpointId::new("fake-runtime"),
        test_timers(),
        DeviceEventSink::noop(),
    )
    .await;

    assert!(matches!(result, Err(crate::LinkError::Other { .. })));
}

// --- helpers -------------------------------------------------------------

fn fake_device_connector(
    script: FakeDeviceScript,
) -> (Rc<LinkConnector>, LinkEndpointId, FakeEsp32Device) {
    let endpoint_id = LinkEndpointId::new("fake-device-0");
    let provider =
        FakeProvider::new().with_device_endpoint(endpoint_id.clone(), "Fake ESP32", script);
    let device = provider.device(&endpoint_id).unwrap();
    (Rc::new(LinkConnector::Fake(provider)), endpoint_id, device)
}

/// Connect + wait_ready against a scripted device with default deadlines.
async fn readiness_outcome(script: FakeDeviceScript) -> DeviceState {
    let (connector, endpoint_id, _device) = fake_device_connector(script);
    let session = DeviceSession::connect(
        connector,
        &endpoint_id,
        test_timers(),
        DeviceEventSink::noop(),
    )
    .await
    .unwrap();
    session.wait_ready().await
}

/// Tokio-backed timers: the test edge's platform sleep.
fn test_timers() -> DeviceTimers {
    DeviceTimers::new(|duration| Box::pin(tokio::time::sleep(duration)))
}

/// Timers with a shortened readiness deadline for deadline-expiry tests.
fn short_ready_timers(ready: Duration) -> DeviceTimers {
    test_timers().with_deadlines(DeviceDeadlines {
        ready,
        ..DeviceDeadlines::default()
    })
}

fn recording_sink() -> (DeviceEventSink, Rc<RefCell<Vec<DeviceEvent>>>) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink_events = Rc::clone(&events);
    let sink = DeviceEventSink::new(move |event| sink_events.borrow_mut().push(event));
    (sink, events)
}

fn recorded_states(events: &Rc<RefCell<Vec<DeviceEvent>>>) -> Vec<DeviceState> {
    events
        .borrow()
        .iter()
        .filter_map(|event| match event {
            DeviceEvent::State { state } => Some(state.clone()),
            _ => None,
        })
        .collect()
}
