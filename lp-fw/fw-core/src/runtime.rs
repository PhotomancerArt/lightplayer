//! Shared firmware runtime loop helpers.
//!
//! Target crates still own boot, hardware setup, scheduling, and yielding. This
//! module only provides the target-neutral parts of a LightPlayer firmware loop:
//! draining client messages and ticking `LpServer` through a `ServerTransport`.

extern crate alloc;

use alloc::vec::Vec;

use lpa_server::{LpServer, ServerError};
use lpc_shared::time::TimeProvider;
use lpc_shared::transport::{Incoming, Link, ServerTransport};
use lpc_wire::{TransportError, WireServerMessage};

/// Result of draining currently available client messages from a transport.
#[derive(Debug)]
pub struct DrainedClientMessages {
    pub messages: Vec<Incoming>,
    pub receive_calls: u32,
    pub error: Option<TransportError>,
}

impl DrainedClientMessages {
    #[must_use]
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }
}

/// Result of one server tick/send step.
#[derive(Debug, Clone)]
pub struct ServerTickOutcome {
    pub delta_ms: u32,
    pub response_count: usize,
    pub frame_time_us: u64,
    pub server_error: Option<ServerError>,
}

/// Send the server's unsolicited hello (id 0) on every link `transport`
/// has open.
///
/// Every embedder loop calls this once, as the first frame it sends when it
/// starts serving (before/with the first heartbeat). The payload is the
/// embedder-injected [`LpServer::hello`]; see
/// `docs/adr/2026-07-14-wire-hello-versioning.md` for the contract.
pub async fn send_unsolicited_hello<T: ServerTransport>(
    server: &LpServer,
    transport: &mut T,
) -> Result<(), TransportError> {
    // A FIXTURE build stays silent, which is exactly what pre-hello
    // firmware looks like — absence of a hello IS the mismatch signal.
    // `lpa-server/fixture-no-hello` refuses the client's Hello REQUEST for
    // the same reason; both halves are needed, because Studio's client
    // asks as well as listening. Never in a released image.
    if cfg!(feature = "fixture-no-hello") {
        return Ok(());
    }
    // Each link reads its own `auth`: a trusted link and an untrusted one
    // on the same device are told different things.
    for link in transport.links() {
        send_hello_to_link(server, transport, link).await?;
    }
    Ok(())
}

/// Send the unsolicited hello (id 0) to one `link` — the one owed to a link
/// that opens after the loop started serving (a radio connection), which
/// [`send_unsolicited_hello`] never saw.
pub async fn send_hello_to_link<T: ServerTransport>(
    server: &LpServer,
    transport: &mut T,
    link: Link,
) -> Result<(), TransportError> {
    // Silent in a FIXTURE build, for `send_unsolicited_hello`'s reason.
    if cfg!(feature = "fixture-no-hello") {
        return Ok(());
    }
    transport
        .send(
            link.id,
            WireServerMessage::new(
                0,
                lpc_wire::server::ServerMsgBody::Hello(server.hello_for_link(link)),
            ),
        )
        .await
}

/// Drain all currently available client messages from `transport`.
///
/// A receive error is returned alongside any messages already collected. This
/// lets target loops decide whether a specific error is fatal.
pub async fn drain_client_messages<T: ServerTransport>(transport: &mut T) -> DrainedClientMessages {
    let mut messages = Vec::new();
    let mut receive_calls = 0;

    loop {
        receive_calls += 1;
        match transport.receive().await {
            Ok(Some(incoming)) => messages.push(incoming),
            Ok(None) => {
                return DrainedClientMessages {
                    messages,
                    receive_calls,
                    error: None,
                };
            }
            Err(error) => {
                return DrainedClientMessages {
                    messages,
                    receive_calls,
                    error: Some(error),
                };
            }
        }
    }
}

/// Tick the server, send responses through `transport`, and record frame time.
pub async fn tick_server_frame<T, P>(
    server: &mut LpServer,
    transport: &mut T,
    time_provider: &P,
    frame_start_ms: u64,
    last_tick_ms: u64,
    incoming_messages: Vec<Incoming>,
) -> ServerTickOutcome
where
    T: ServerTransport,
    P: TimeProvider,
{
    let delta_time = time_provider.elapsed_ms(last_tick_ms);
    let delta_ms = delta_time.min(u32::MAX as u64) as u32;
    let delta_ms = delta_ms.max(1);

    match server
        .tick_and_send(delta_ms, incoming_messages, transport)
        .await
    {
        Ok(response_count) => {
            let frame_time_us = elapsed_us(time_provider, frame_start_ms);
            server.set_last_frame_time(frame_time_us);
            ServerTickOutcome {
                delta_ms,
                response_count,
                frame_time_us,
                server_error: None,
            }
        }
        Err(error) => {
            let frame_time_us = elapsed_us(time_provider, frame_start_ms);
            server.set_last_frame_time(frame_time_us);
            ServerTickOutcome {
                delta_ms,
                response_count: 0,
                frame_time_us,
                server_error: Some(error),
            }
        }
    }
}

fn elapsed_us<P: TimeProvider>(time_provider: &P, start_ms: u64) -> u64 {
    time_provider.elapsed_ms(start_ms).saturating_mul(1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::FakeTransport;
    use lpc_shared::time::TimeProvider;
    use lpc_wire::{ClientMessage, ClientRequest};

    #[test]
    fn drain_client_messages_collects_until_empty() {
        let mut transport = FakeTransport::new();
        transport.queue_message(ClientMessage {
            id: 1,
            msg: ClientRequest::ListAvailableProjects,
        });
        transport.queue_message(ClientMessage {
            id: 2,
            msg: ClientRequest::ListLoadedProjects,
        });

        let drained = pollster::block_on(drain_client_messages(&mut transport));

        assert_eq!(drained.message_count(), 2);
        assert_eq!(drained.receive_calls, 3);
        assert!(drained.error.is_none());
    }

    /// The boot hello goes to every open link, each carrying that link's
    /// own `auth`: a trusted link is told edit, a fresh radio link nothing.
    #[test]
    fn the_hello_goes_to_every_link_with_its_own_auth() {
        use alloc::boxed::Box;
        use alloc::rc::Rc;
        use alloc::sync::Arc;
        use core::cell::RefCell;
        use lpc_model::AsLpPath;
        use lpc_shared::transport::{Link, LinkId, LinkTrust};

        let radio = Link {
            id: LinkId::new(3),
            trust: LinkTrust::Untrusted,
        };
        let server = LpServer::new(
            Rc::new(RefCell::new(lpc_shared::output::MemoryOutputProvider::new())),
            Box::new(lpfs::LpFsMemory::new()),
            "/projects/".as_path(),
            None,
            None,
            Arc::new(lp_gfx::NullGraphics::new()),
        );
        let mut transport = TwoLinks {
            links: alloc::vec![Link::PRIMARY, radio],
            sent: Vec::new(),
        };

        pollster::block_on(send_unsolicited_hello(&server, &mut transport)).unwrap();

        let auths: Vec<_> = transport
            .sent
            .iter()
            .map(|(link, msg)| match &msg.msg {
                lpc_wire::server::ServerMsgBody::Hello(hello) => (*link, hello.auth),
                other => panic!("not a hello: {other:?}"),
            })
            .collect();
        assert_eq!(
            auths,
            alloc::vec![
                (LinkId::PRIMARY, lpc_wire::HelloAuth::TRUSTED),
                (
                    radio.id,
                    lpc_wire::HelloAuth {
                        required: true,
                        granted: None
                    }
                ),
            ]
        );
    }

    struct TwoLinks {
        links: Vec<lpc_shared::transport::Link>,
        sent: Vec<(lpc_shared::transport::LinkId, WireServerMessage)>,
    }

    impl ServerTransport for TwoLinks {
        async fn send(
            &mut self,
            link: lpc_shared::transport::LinkId,
            msg: WireServerMessage,
        ) -> Result<(), TransportError> {
            self.sent.push((link, msg));
            Ok(())
        }

        async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
            Ok(None)
        }

        async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
            Ok(Vec::new())
        }

        fn links(&self) -> Vec<lpc_shared::transport::Link> {
            self.links.clone()
        }

        async fn close(&mut self) -> Result<(), TransportError> {
            Ok(())
        }
    }

    #[test]
    fn mock_time_provider_reports_elapsed_ms() {
        let time = MockTimeProvider { now_ms: 42 };

        assert_eq!(time.elapsed_ms(40), 2);
    }

    struct MockTimeProvider {
        now_ms: u64,
    }

    impl TimeProvider for MockTimeProvider {
        fn now_ms(&self) -> u64 {
            self.now_ms
        }
    }
}
