//! Unified CLI connection: every host kind as a [`ClientIo`].
//!
//! Hardware serial devices connect through a real [`DeviceSession`]
//! (wire-hello readiness, mode-exclusive management, reconnect-that-rebuilds)
//! — the same abstraction the studio uses. Non-device hosts (websocket,
//! local process, emulator) are plain transports wrapped as `ClientIo`.
//!
//! Everything here is `!Send` (the DeviceSession world is single-actor by
//! design): callers run on a current-thread runtime + `LocalSet` and speak
//! the runtime-neutral [`lpa_client::LpClient`].

use std::rc::Rc;

use anyhow::{Context, Result};
use lpa_client::{ClientIo, TokioClientIo, WebSocketClientTransport};
use lpa_link::providers::host_serial_esp32::{
    HostSerialEsp32Options, HostSerialEsp32Provider, label_for_port,
};
use lpa_link::{
    DeviceEvent, DeviceEventSink, DeviceLineOrigin, DeviceSession, DeviceState, DeviceTimers,
    LinkConnector,
};

use crate::client::HostSpecifier;
use crate::client::client_connect::client_connect;
use crate::client::serial_port::detect_serial_port;

/// An open CLI connection; owns whatever keeps the link alive.
pub enum CliConnection {
    /// Hardware device behind a [`DeviceSession`] (owns the session).
    Device(DeviceSession),
    /// Non-device host: a plain shared transport.
    Transport(TokioClientIo),
}

impl CliConnection {
    /// The app-protocol channel for [`lpa_client::LpClient`].
    pub fn client_io(&self) -> Box<dyn ClientIo> {
        match self {
            Self::Device(session) => session.client_io(),
            Self::Transport(io) => Box::new(io.clone()),
        }
    }

    pub async fn close(self) {
        match self {
            Self::Device(session) => {
                let _ = session.close().await;
            }
            Self::Transport(io) => {
                let transport = io.shared_transport();
                let mut transport = transport.lock().await;
                let _ = lpa_client::ClientTransport::close(&mut **transport).await;
            }
        }
    }
}

/// Connect `spec` and wait for readiness where the host is a device.
///
/// Serial devices are reset on connect: readiness is granted only by the
/// boot [`ServerHello`](lpc_wire::ServerHello), so the session watches a
/// fresh boot rather than assuming whatever state the device was in.
/// `on_event` observes the session feed (console lines, state transitions);
/// pass [`DeviceEventSink::noop`]-like behavior by ignoring events.
pub async fn cli_connect(
    spec: HostSpecifier,
    on_event: impl Fn(DeviceEvent) + 'static,
) -> Result<CliConnection> {
    match spec {
        HostSpecifier::Serial { port, baud_rate } => {
            let config = detect_serial_port(port.as_deref(), baud_rate)
                .context("Failed to detect serial port")?;
            let provider = HostSerialEsp32Provider::with_options(HostSerialEsp32Options {
                baud_rate: Some(config.baud_rate),
                reset_after_open: true,
                ..HostSerialEsp32Options::default()
            });
            let endpoint_id =
                provider.create_endpoint_for_port(&config.port, label_for_port(&config.port));
            let connector = Rc::new(LinkConnector::HostSerialEsp32(provider));
            let timers = DeviceTimers::new(|duration| Box::pin(tokio::time::sleep(duration)));
            let session = DeviceSession::connect(
                connector,
                &endpoint_id,
                timers,
                DeviceEventSink::new(on_event),
            )
            .await
            .map_err(|error| anyhow::anyhow!("failed to open device session: {error}"))?;
            let state = session.wait_ready().await;
            if !state.is_ready() {
                let message = state
                    .unavailable_message()
                    .unwrap_or_else(|| format!("{state:?}"));
                let _ = session.close().await;
                anyhow::bail!("device did not become ready: {message}");
            }
            Ok(CliConnection::Device(session))
        }
        HostSpecifier::WebSocket { url } => {
            let transport = WebSocketClientTransport::new(&url)
                .await
                .map_err(|error| anyhow::anyhow!("Failed to connect to {url}: {error}"))?;
            Ok(CliConnection::Transport(TokioClientIo::new(Box::new(
                transport,
            ))))
        }
        // Local process + emulator: reuse the existing sync constructors.
        other => {
            let transport = client_connect(other)?;
            Ok(CliConnection::Transport(TokioClientIo::new(transport)))
        }
    }
}

/// Route a [`DeviceEvent`] feed to stderr: device console lines when
/// `verbose`, link/log lines and state transitions always.
pub fn stderr_device_events(verbose: bool) -> impl Fn(DeviceEvent) + 'static {
    move |event| match event {
        DeviceEvent::LogLine { line, origin } => {
            if verbose || origin == DeviceLineOrigin::Link {
                eprintln!("[device] {line}");
            }
        }
        DeviceEvent::State { state } => {
            if verbose || !matches!(state, DeviceState::Booting | DeviceState::Ready { .. }) {
                eprintln!("[device] state: {state:?}");
            }
        }
        DeviceEvent::Progress { label, percent } => match percent {
            Some(percent) => eprintln!("[device] {label}: {percent}%"),
            None => eprintln!("[device] {label}"),
        },
    }
}
