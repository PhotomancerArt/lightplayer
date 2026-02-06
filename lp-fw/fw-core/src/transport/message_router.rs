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
    fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        // Serialize to JSON
        let json = json::to_string(&msg).map_err(|e| {
            TransportError::Serialization(format!("Failed to serialize ServerMessage: {e}"))
        })?;

        // Add M! prefix and newline
        let message = format!("M!{json}\n");

        // Send via router (non-blocking)
        self.router.send(message).map_err(|_| {
            TransportError::Other(format!("MessageRouter send error: channel full"))
        })?;

        log::debug!(
            "MessageRouterTransport: Sent message id={} via router",
            msg.id
        );

        Ok(())
    }

    fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        // Receive all available messages from router
        let messages = self.router.receive_all();

        // Process first valid message
        for msg_line in messages {
            // Check for M! prefix
            if !msg_line.starts_with("M!") {
                log::trace!("MessageRouterTransport: Skipping non-message line (no M! prefix)");
                continue;
            }

            // Extract JSON (skip M! prefix and trim newline)
            let json_str = msg_line.strip_prefix("M!").unwrap_or(&msg_line);
            let json_str = json_str.trim_end_matches('\n');

            // Parse JSON
            match json::from_str::<ClientMessage>(json_str) {
                Ok(msg) => {
                    log::debug!(
                        "MessageRouterTransport: Received message id={} via router",
                        msg.id
                    );
                    return Ok(Some(msg));
                }
                Err(e) => {
                    log::warn!("MessageRouterTransport: Failed to parse JSON message: {e}");
                    continue;
                }
            }
        }

        // No messages available
        Ok(None)
    }

    fn close(&mut self) -> Result<(), TransportError> {
        // MessageRouter doesn't need explicit closing
        // Channels will be dropped when router is dropped
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use crate::message_router::MessageRouter;
    use alloc::string::ToString;
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::Channel;
    use lp_model::ClientRequest;

    static TEST_INCOMING: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();
    static TEST_OUTGOING: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();

    #[test]
    fn test_send_message() {
        // Clear channels
        while TEST_OUTGOING.receiver().try_receive().is_ok() {}
        while TEST_INCOMING.receiver().try_receive().is_ok() {}

        let router = MessageRouter::new(&TEST_INCOMING, &TEST_OUTGOING);
        let mut transport = MessageRouterTransport::new(router);

        let msg = ServerMessage {
            id: 1,
            msg: lp_model::server::ServerMsgBody::UnloadProject,
        };
        transport.send(msg).unwrap();

        // Check message was sent to router
        let router_msg = TEST_OUTGOING.receiver().try_receive().unwrap();
        assert!(
            router_msg.starts_with("M!"),
            "Message should start with M! prefix"
        );
        assert!(router_msg.contains("\"unloadProject\""));
        assert!(router_msg.ends_with('\n'));
    }

    #[test]
    fn test_receive_message() {
        // Clear channels
        while TEST_OUTGOING.receiver().try_receive().is_ok() {}
        while TEST_INCOMING.receiver().try_receive().is_ok() {}

        let router = MessageRouter::new(&TEST_INCOMING, &TEST_OUTGOING);
        let mut transport = MessageRouterTransport::new(router);

        // Push message to router
        TEST_INCOMING
            .sender()
            .try_send("M!{\"id\":1,\"msg\":{\"loadProject\":{\"path\":\"test\"}}}\n".to_string())
            .unwrap();

        // Receive via transport
        let msg = transport.receive().unwrap().unwrap();
        assert_eq!(msg.id, 1);
        match msg.msg {
            ClientRequest::LoadProject { path } => assert_eq!(path, "test"),
            _ => panic!("Unexpected message type"),
        }
    }

    #[test]
    fn test_receive_filters_non_message_lines() {
        // Clear channels
        while TEST_OUTGOING.receiver().try_receive().is_ok() {}
        while TEST_INCOMING.receiver().try_receive().is_ok() {}

        let router = MessageRouter::new(&TEST_INCOMING, &TEST_OUTGOING);
        let mut transport = MessageRouterTransport::new(router);

        // Push non-message line
        TEST_INCOMING
            .sender()
            .try_send("debug: some log output\n".to_string())
            .unwrap();

        // Should return None (filtered out)
        assert!(transport.receive().unwrap().is_none());
    }
}
