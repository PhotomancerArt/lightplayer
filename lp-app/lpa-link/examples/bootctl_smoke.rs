//! Manual hardware smoke for the boot-control sector, end to end.
//!
//! Drives a real board through the recovery escape the way Studio does:
//! connect → Ready → `SetBootControl(skip project auto-load)` → the device
//! reboots and comes up reachable with nothing loaded → reboot again → the
//! record is gone and the project loads normally.
//!
//! That last step is the point. A boot-control record is one-shot; the
//! firmware consumes it as it reads it, so "safe once" must not become "safe
//! forever". It is also the step easiest to skip by hand.
//!
//! ```sh
//! cargo run -p lpa-link --features host-serial-esp32 --example bootctl_smoke -- \
//!     /dev/cu.usbmodem1101
//! ```
//!
//! NON-DESTRUCTIVE: writes a 16-byte record into the `bootctl` partition and
//! reboots. Nothing is erased, and the device's project survives.
//!
//! Companion to `manage_smoke`, which covers the erase/flash/reset cycle and
//! IS destructive.

use std::rc::Rc;
use std::time::Duration;

use lpa_link::providers::host_serial_esp32::{
    HostSerialEsp32Options, HostSerialEsp32Provider, label_for_port,
};
use lpa_link::{
    DeviceDeadlines, DeviceEvent, DeviceEventSink, DeviceSession, DeviceTimers, LinkConnector,
    LinkManagementRequest, LinkManagementResult,
};

fn event_printer() -> DeviceEventSink {
    DeviceEventSink::new(|event| match event {
        DeviceEvent::LogLine { line, .. } => println!("  | {line}"),
        DeviceEvent::State { to: state, .. } => println!("  * state: {state:?}"),
        DeviceEvent::Progress { label, percent } => match percent {
            Some(percent) => println!("  % {label}: {percent}%"),
            None => println!("  % {label}"),
        },
        DeviceEvent::ParseAnomaly { detail } => println!("  ! parse anomaly: {detail}"),
        DeviceEvent::TxFrame { .. } => {}
    })
}

fn main() {
    let port = std::env::args()
        .nth(1)
        .expect("usage: bootctl_smoke <port>");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(run(port)));
}

async fn run(port: String) {
    let provider = HostSerialEsp32Provider::with_options(HostSerialEsp32Options {
        reset_after_open: true,
        ..HostSerialEsp32Options::default()
    });
    let endpoint_id = provider.create_endpoint_for_port(&port, label_for_port(&port));
    let connector = Rc::new(LinkConnector::HostSerialEsp32(provider));
    let timers = DeviceTimers::new(|duration| Box::pin(tokio::time::sleep(duration)))
        .with_deadlines(DeviceDeadlines {
            ready: Duration::from_secs(30),
            ..DeviceDeadlines::default()
        });

    println!("== connect ==");
    let session = DeviceSession::connect(connector, &endpoint_id, timers, event_printer())
        .await
        .expect("device session connect");
    let state = session.wait_ready().await;
    assert!(
        state.is_ready(),
        "expected Ready after connect, got {state:?}"
    );

    println!("\n== manage: SetBootControl (safe mode) ==");
    let outcome = session
        .manage(LinkManagementRequest::start_safe_mode(), event_printer())
        .await
        .expect("write boot-control record");
    match &outcome.result {
        LinkManagementResult::SetBootControl(result) => {
            println!(
                "wrote flags {:#010x} to {:?}",
                result.flags, result.chip_name
            );
            assert_ne!(result.flags, 0, "the request must carry an instruction");
        }
        other => panic!("expected SetBootControl result, got {other:?}"),
    }
    assert!(
        outcome.state.is_ready(),
        "the device must come back reachable after a boot-control write \
         (nothing was erased), got {:?}",
        outcome.state
    );

    println!(
        "\n>> Expect the device's boot log to carry:\n\
         >>   [BOOTCTL] record found: flags=0x00000001\n\
         >>   [BOOTCTL] record consumed\n\
         >>   [BOOTCTL] SAFE BOOT: boot-control record — skipping project auto-load"
    );

    println!("\n== manage: ResetRuntime (second boot — the record must be GONE) ==");
    let outcome = session
        .manage(LinkManagementRequest::ResetRuntime, event_printer())
        .await
        .expect("reset runtime");
    assert!(
        outcome.state.is_ready(),
        "expected Ready after the second reboot, got {:?}",
        outcome.state
    );
    println!(
        "\n>> The second boot log must NOT carry a [BOOTCTL] SAFE BOOT line.\n\
         >> If it does, the one-shot consume is broken and 'safe once' has\n\
         >> become 'safe forever'."
    );

    println!("\n== bootctl smoke complete ==");
}
