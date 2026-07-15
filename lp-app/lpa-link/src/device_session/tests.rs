//! DeviceSession state-machine tests against the scripted fake ESP32.
//!
//! Test edges may block and use tokio timers; the session itself only sees
//! the injected [`DeviceTimers`] factory.

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

use lpc_wire::{ClientMessage, ClientRequest, WIRE_PROTO_VERSION};

use crate::provider::endpoint::LinkEndpointId;
use crate::providers::fake::FakeProvider;
use crate::providers::fake_device::{
    FAKE_IMAGE_IDENTITY, FakeBootState, FakeDeviceIdentity, FakeDeviceScript, FakeEsp32Device,
    FakeFailurePlan, FakeLightPlayerState,
};
use crate::{
    LinkConnector, LinkEndpointStatus, LinkManagementRequest, LinkManagementResult,
    LinkSessionStatus,
};

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
            .any(|event| matches!(event, DeviceEvent::LogLine { line, origin: DeviceLineOrigin::Device } if line.contains("starting server loop"))),
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

// --- P3: management + reconnect ------------------------------------------

#[tokio::test]
async fn flash_rebuilds_the_link_and_readiness_lands_ready_with_new_provenance() {
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
    let DeviceState::Ready { hello } = session.wait_ready().await else {
        panic!("expected Ready before flashing");
    };
    assert_eq!(hello.fw.commit, "fake-firmware");
    let (sink, events) = recording_sink();

    let outcome = session
        .manage(LinkManagementRequest::FlashFirmware, sink)
        .await
        .unwrap();

    assert!(matches!(
        outcome.result,
        LinkManagementResult::FlashFirmware(_)
    ));
    let DeviceState::Ready { hello } = outcome.state else {
        panic!("expected Ready after flash, got {:?}", outcome.state);
    };
    assert_eq!(
        hello.fw.commit, FAKE_IMAGE_IDENTITY,
        "the rebuilt link's hello carries the flashed image's provenance"
    );
    assert_eq!(session.mode(), DeviceMode::AppProtocol);
    assert_eq!(session.session().status, LinkSessionStatus::Open);
    // The connector's management events reached the sink FOLDED into the
    // DeviceEvent vocabulary: logs as LogLine, progress as Progress.
    assert!(
        events.borrow().iter().any(
            |event| matches!(event, DeviceEvent::LogLine { line, origin: DeviceLineOrigin::Link } if line.contains("fake flash"))
        ),
        "management logs should arrive as LogLine events"
    );
    assert!(
        events.borrow().iter().any(|event| matches!(
            event,
            DeviceEvent::Progress { label, percent: Some(100) } if label == "Firmware written"
        )),
        "management progress should arrive as Progress events"
    );
    // The rebuilt channel speaks the app protocol again.
    let mut client = lpa_client::LpClient::new(session.client_io());
    assert!(client.hello().await.is_ok());
}

#[tokio::test]
async fn erase_lands_blank_flash_and_that_is_success() {
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

    let outcome = session
        .manage(
            LinkManagementRequest::EraseDeviceFlash,
            DeviceEventSink::noop(),
        )
        .await
        .unwrap();

    assert!(matches!(
        outcome.result,
        LinkManagementResult::EraseDeviceFlash(_)
    ));
    assert_eq!(outcome.state, DeviceState::BlankFlash);
    // BlankFlash is where a successful erase LANDS — the session record
    // stays healthy (no Error status) and the mode is released.
    assert_eq!(session.session().status, LinkSessionStatus::Open);
    assert_eq!(session.mode(), DeviceMode::AppProtocol);
}

#[tokio::test]
async fn reset_replays_the_boot_and_lands_ready_again() {
    let (connector, endpoint_id, _device) =
        fake_device_connector(FakeDeviceScript::new(FakeBootState::LightPlayer(
            FakeLightPlayerState::new()
                .with_identity(FakeDeviceIdentity::new("dev_fakefakefakefak0", "Bench")),
        )));
    let session = DeviceSession::connect(
        connector,
        &endpoint_id,
        test_timers(),
        DeviceEventSink::noop(),
    )
    .await
    .unwrap();
    assert!(session.wait_ready().await.is_ready());

    let outcome = session
        .manage(LinkManagementRequest::ResetRuntime, DeviceEventSink::noop())
        .await
        .unwrap();

    assert!(matches!(outcome.result, LinkManagementResult::ResetRuntime));
    let DeviceState::Ready { hello } = outcome.state else {
        panic!("expected Ready after reset, got {:?}", outcome.state);
    };
    assert_eq!(hello.device_uid.as_deref(), Some("dev_fakefakefakefak0"));
}

#[tokio::test]
async fn scripted_manage_failure_sets_error_status_and_reconnect_recovers() {
    let (connector, endpoint_id, _device) = fake_device_connector(
        FakeDeviceScript::new(FakeBootState::LightPlayer(FakeLightPlayerState::new()))
            .with_manage_failure("scripted flash tool failure"),
    );
    let session = DeviceSession::connect(
        connector,
        &endpoint_id,
        test_timers(),
        DeviceEventSink::noop(),
    )
    .await
    .unwrap();
    assert!(session.wait_ready().await.is_ready());

    let error = session
        .manage(
            LinkManagementRequest::FlashFirmware,
            DeviceEventSink::noop(),
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("scripted flash tool failure"));
    // Error statuses populated; the wire was released, so the state is Gone.
    assert_eq!(session.state(), DeviceState::Gone);
    assert!(matches!(
        session.session().status,
        LinkSessionStatus::Error { ref message } if message.contains("scripted flash tool failure")
    ));
    assert!(matches!(
        session.snapshot().endpoint_status,
        LinkEndpointStatus::Error { .. }
    ));
    // Mode restored; the channel errors cleanly until a reconnect.
    assert_eq!(session.mode(), DeviceMode::AppProtocol);
    let mut io = session.client_io();
    assert!(
        io.send(ClientMessage {
            id: 1,
            msg: ClientRequest::Hello,
        })
        .await
        .is_err()
    );

    // Reconnect = rebuild: the same handle (and the SAME already-handed-out
    // channel) becomes usable again on the new link generation.
    let state = session.reconnect().await.unwrap();

    assert!(state.is_ready());
    assert_eq!(session.session().status, LinkSessionStatus::Open);
    let mut client = lpa_client::LpClient::new(session.client_io());
    assert!(client.hello().await.is_ok());
}

#[tokio::test]
async fn manage_success_with_failed_rebuild_keeps_the_result_and_lands_gone() {
    // P5 regression: a rebuild failure after a SUCCESSFUL operation must not
    // swallow the operation's result — the erase happened on the device.
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
    // Arm the failure AFTER connect: only the post-management rebuild's
    // fresh `connect` sees it.
    let LinkConnector::Fake(provider) = &*connector else {
        panic!("fake connector");
    };
    provider.set_connect_error(Some("port grabbed by another tool".to_string()));
    let (sink, events) = recording_sink();

    let outcome = session
        .manage(LinkManagementRequest::EraseDeviceFlash, sink)
        .await
        .expect("a successful erase with a failed rebuild is still a result");

    assert!(matches!(
        outcome.result,
        LinkManagementResult::EraseDeviceFlash(_)
    ));
    assert_eq!(outcome.state, DeviceState::Gone);
    assert_eq!(session.state(), DeviceState::Gone);
    assert!(matches!(
        session.session().status,
        LinkSessionStatus::Error { ref message } if message.contains("rebuild failed")
    ));
    assert!(
        events.borrow().iter().any(|event| matches!(
            event,
            DeviceEvent::LogLine { line, origin: DeviceLineOrigin::Link }
                if line.contains("rebuild failed after management")
        )),
        "the rebuild failure surfaces on the management sink"
    );

    // Recovery stays the normal reconnect once the port frees up.
    provider.set_connect_error(None);
    assert_eq!(session.reconnect().await.unwrap(), DeviceState::BlankFlash);
}

#[tokio::test]
async fn reconnect_rebuilds_a_gone_session() {
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
    assert_eq!(session.wait_ready().await, DeviceState::Gone);

    // The device comes back (unplug/replug): recovery is a plain reconnect.
    device.set_failure_plan(FakeFailurePlan::none());
    let state = session.reconnect().await.unwrap();

    assert!(state.is_ready());
    assert_eq!(session.session().status, LinkSessionStatus::Open);
}

#[tokio::test]
async fn manage_is_refused_while_a_request_is_in_flight() {
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

    // Park a receive mid-flight: no response frame is coming, so a single
    // poll leaves the request pending inside the transport wait.
    let mut io = session.client_io();
    let mut receive = Box::pin(io.receive());
    assert!(
        poll_once_noop(receive.as_mut()).is_pending(),
        "receive should be parked waiting for a frame"
    );

    let error = session
        .manage(LinkManagementRequest::ResetRuntime, DeviceEventSink::noop())
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("in flight"),
        "manage must refuse cleanly mid-request: {error}"
    );

    // Dropping the request releases the wire for management.
    drop(receive);
    let guard = session.try_begin_management().unwrap();
    drop(guard);
}

#[tokio::test]
async fn stale_blank_flash_lines_do_not_misclassify_the_post_flash_rebuild() {
    // The M3 lesson: the blank-flash boot lines observed BEFORE the flash
    // must not classify the rebuilt link, or a successful flash would read
    // as still-blank.
    let (connector, endpoint_id, _device) =
        fake_device_connector(FakeDeviceScript::new(FakeBootState::BlankFlash));
    let session = DeviceSession::connect(
        connector,
        &endpoint_id,
        test_timers(),
        DeviceEventSink::noop(),
    )
    .await
    .unwrap();
    assert_eq!(session.wait_ready().await, DeviceState::BlankFlash);
    assert!(
        session
            .snapshot()
            .recent_lines
            .iter()
            .any(|line| line.contains("invalid header")),
        "the blank-flash diagnosis should have observed invalid-header lines"
    );

    let outcome = session
        .manage(
            LinkManagementRequest::FlashFirmware,
            DeviceEventSink::noop(),
        )
        .await
        .unwrap();

    assert!(
        outcome.state.is_ready(),
        "flash from blank must land Ready, got {:?}",
        outcome.state
    );
    assert!(
        !session
            .snapshot()
            .recent_lines
            .iter()
            .any(|line| line.contains("invalid header")),
        "stale blank-flash lines must be cleared across the rebuild"
    );
}

// --- helpers -------------------------------------------------------------

/// Poll a pinned future exactly once with a no-op waker.
fn poll_once_noop<F: Future>(future: std::pin::Pin<&mut F>) -> Poll<F::Output> {
    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    future.poll(&mut context)
}

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
