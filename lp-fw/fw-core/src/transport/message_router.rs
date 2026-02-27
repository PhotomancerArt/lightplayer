//! MessageRouter-based transport implementation
//!
//! Wraps `MessageRouter` and implements `ServerTransport` trait.
//! Converts between `String` messages (router) and `ClientMessage`/`ServerMessage` (transport).

extern crate alloc;

use alloc::format;

use crate::message_router::MessageRouter;
use lp_model::{ClientMessage, ServerMessage, TransportError, json};
use lp_shared::transport::ServerTransport;

/// Transport implementation using MessageRouter
///
/// Wraps a `MessageRouter` and implements `ServerTransport` by converting
/// between `String` messages (used by router) and `ClientMessage`/`ServerMessage`
/// (used by transport interface).
pub struct MessageRouterTransport {
    /// Message router for task communication
    router: MessageRouter,
}

impl MessageRouterTransport {
    /// Create a new MessageRouterTransport with the given router
    pub fn new(router: MessageRouter) -> Self {
        Self { router }
    }
}

impl ServerTransport for MessageRouterTransport {
    async fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        let json = json::to_string(&msg).map_err(|e| {
            TransportError::Serialization(format!("Failed to serialize ServerMessage: {e}"))
        })?;
        let message = alloc::format!("M!{json}\n");
        self.router.send(message).map_err(|_| {
            TransportError::Other(alloc::format!("MessageRouter send error: channel full"))
        })?;
        log::debug!(
            "MessageRouterTransport: Sent message id={} via router",
            msg.id
        );
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        let messages = self.router.receive_all();
        for msg_line in messages {
            if !msg_line.starts_with("M!") {
                continue;
            }
            let json_str = msg_line.strip_prefix("M!").unwrap_or(&msg_line);
            let json_str = json_str.trim_end_matches('\n');
            match json::from_str::<ClientMessage>(json_str) {
                Ok(msg) => {
                    log::debug!(
                        "MessageRouterTransport: Received message id={} via router",
                        msg.id
                    );
                    return Ok(Some(msg));
                }
                Err(e) => {
                    log::warn!(
                        "MessageRouterTransport: Failed to parse JSON message: {e} | json: {json_str}"
                    );
                    continue;
                }
            }
        }
        Ok(None)
    }

    async fn receive_all(&mut self) -> Result<alloc::vec::Vec<ClientMessage>, TransportError> {
        let messages = self.router.receive_all();
        let mut result = alloc::vec::Vec::new();
        for msg_line in messages {
            if !msg_line.starts_with("M!") {
                continue;
            }
            let json_str = msg_line.strip_prefix("M!").unwrap_or(&msg_line);
            let json_str = json_str.trim_end_matches('\n');
            if let Ok(msg) = json::from_str::<ClientMessage>(json_str) {
                result.push(msg);
            }
        }
        Ok(result)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use crate::message_router::MessageRouter;
    use alloc::{
        boxed::Box,
        string::{String, ToString},
    };
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::Channel;
    use lp_model::ClientRequest;

    /// Helper to create a router with fresh channels for each test
    fn create_test_router() -> (
        MessageRouter,
        &'static Channel<CriticalSectionRawMutex, String, 32>,
        &'static Channel<CriticalSectionRawMutex, String, 32>,
    ) {
        let incoming = Box::leak(Box::new(Channel::new()));
        let outgoing = Box::leak(Box::new(Channel::new()));
        let router = MessageRouter::new(incoming, outgoing);
        (router, incoming, outgoing)
    }

    #[test]
    fn test_send_message() {
        let (router, _, outgoing) = create_test_router();
        let mut transport = MessageRouterTransport::new(router);

        let msg = ServerMessage {
            id: 1,
            msg: lp_model::server::ServerMsgBody::UnloadProject,
        };
        pollster::block_on(transport.send(msg)).unwrap();

        // Check message was sent to router
        let router_msg = outgoing.receiver().try_receive().unwrap();
        assert!(
            router_msg.starts_with("M!"),
            "Message should start with M! prefix"
        );
        assert!(router_msg.contains("\"unloadProject\""));
        assert!(router_msg.ends_with('\n'));
    }

    #[test]
    fn test_receive_message() {
        let (router, incoming, _outgoing) = create_test_router();
        let mut transport = MessageRouterTransport::new(router);

        // Push message to router
        incoming
            .sender()
            .try_send("M!{\"id\":1,\"msg\":{\"loadProject\":{\"path\":\"test\"}}}\n".to_string())
            .unwrap();

        // Receive via transport
        let msg = pollster::block_on(transport.receive()).unwrap().unwrap();
        assert_eq!(msg.id, 1);
        match msg.msg {
            ClientRequest::LoadProject { path } => assert_eq!(path, "test"),
            _ => panic!("Unexpected message type"),
        }
    }

    #[test]
    fn test_receive_filters_non_message_lines() {
        let (router, incoming, _outgoing) = create_test_router();
        let mut transport = MessageRouterTransport::new(router);

        // Push non-message line
        incoming
            .sender()
            .try_send("debug: some log output\n".to_string())
            .unwrap();

        // Should return None (filtered out)
        assert!(pollster::block_on(transport.receive()).unwrap().is_none());
    }
}
