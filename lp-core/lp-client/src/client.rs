//! Client struct for communicating with LpServer
//!
//! Provides async request/response correlation and handles server-initiated messages
//! (like log messages) via a background receiver task.

extern crate alloc;

use crate::channel::{self, UnboundedReceiver, UnboundedSender, oneshot};
use crate::error::ClientError;
use crate::transport::client::ClientTransport;
use alloc::{boxed::Box, format, string::ToString, sync::Arc};
use core::sync::atomic::{AtomicBool, Ordering};
use hashbrown::HashMap;
use lp_model::{
    ClientRequest, TransportError,
    message::{ClientMessage, ServerMessage},
};

/// Shared state for the background receiver task
struct ReceiverState {
    /// Map of request ID -> oneshot sender for response
    pending_requests: HashMap<u64, oneshot::OneshotSender<Result<ServerMessage, TransportError>>>,
    /// Channel for non-response messages (log messages, etc.)
    non_response_tx: UnboundedSender<ServerMessage>,
    /// Closed flag
    closed: Arc<AtomicBool>,
}

/// Client for communicating with LpServer
///
/// Manages request/response correlation and routes server-initiated messages
/// (like log messages) to a separate queue. Uses a background task that owns
/// the transport and handles both sending and receiving.
pub struct LpClient {
    /// Channel for sending requests to the receiver task
    request_tx: UnboundedSender<(
        ClientMessage,
        oneshot::OneshotSender<Result<ServerMessage, TransportError>>,
    )>,

    /// Next request ID to use (wrapped in Arc for interior mutability)
    next_request_id: Arc<core::sync::atomic::AtomicU64>,

    /// Receiver for non-response messages
    non_response_rx: UnboundedReceiver<ServerMessage>,

    /// Closed flag (shared with receiver task)
    closed: Arc<AtomicBool>,
}

/// Components needed to spawn the receiver task
pub struct ReceiverTaskComponents {
    /// The transport (will be moved into the task)
    pub transport: Box<dyn ClientTransport>,
    /// Receiver for requests from send_request()
    pub request_rx: UnboundedReceiver<(
        ClientMessage,
        oneshot::OneshotSender<Result<ServerMessage, TransportError>>,
    )>,
    /// Shared state for routing messages
    pub receiver_state: Arc<ReceiverState>,
}

impl LpClient {
    /// Create a new LpClient with the given transport
    ///
    /// Returns both the client and the components needed to spawn the receiver task.
    /// The caller must spawn the receiver task using `receiver_task()` method
    /// to start receiving messages from the transport.
    ///
    /// # Arguments
    ///
    /// * `transport` - The client transport to use for communication
    ///
    /// # Returns
    ///
    /// * `(LpClient, ReceiverTaskComponents)` - The client and receiver task components
    pub fn new(transport: Box<dyn ClientTransport>) -> (Self, ReceiverTaskComponents) {
        // Create channels
        let (request_tx, request_rx) = channel::unbounded();
        let (non_response_tx, non_response_rx) = channel::unbounded();

        // Create shared state
        let receiver_state = Arc::new(ReceiverState {
            pending_requests: HashMap::new(),
            non_response_tx,
            closed: Arc::new(AtomicBool::new(false)),
        });

        let client = Self {
            request_tx,
            next_request_id: Arc::new(core::sync::atomic::AtomicU64::new(1)),
            non_response_rx,
            closed: Arc::clone(&receiver_state.closed),
        };

        let components = ReceiverTaskComponents {
            transport,
            request_rx,
            receiver_state,
        };

        (client, components)
    }

    /// Run the receiver task
    ///
    /// This should be spawned as a background task in your async runtime.
    /// It continuously receives messages from the transport and routes them to
    /// either pending requests or the non-response queue.
    ///
    /// # Arguments
    ///
    /// * `components` - The receiver task components from `new()`
    pub async fn receiver_task(components: ReceiverTaskComponents) {
        let ReceiverTaskComponents {
            mut transport,
            mut request_rx,
            receiver_state,
        } = components;

        loop {
            // Check if closed
            if receiver_state.closed.load(Ordering::Relaxed) {
                break;
            }

            // Process outgoing requests first (non-blocking)
            use core::pin::Pin;
            use core::task::{Context, Poll};

            // Create a no-op waker for polling
            struct NoOpWaker;
            impl core::task::Wake for NoOpWaker {
                fn wake(self: Arc<Self>) {}
            }
            let waker = Arc::new(NoOpWaker).into();
            let mut cx = Context::from_waker(&waker);

            // Try to get requests (non-blocking, drain all available)
            let mut request_rx_pinned = Pin::new(&mut request_rx);
            let mut processed_any = false;
            loop {
                match request_rx_pinned.as_mut().poll(&mut cx) {
                    Poll::Ready(Some((client_msg, response_tx))) => {
                        let id = client_msg.id;

                        // Send via transport
                        if let Err(e) = transport.send(client_msg).await {
                            // Send error to response channel
                            let _ = response_tx.send(Err(e));
                            continue;
                        }

                        // Track the pending request
                        receiver_state.pending_requests.insert(id, response_tx);
                        processed_any = true;
                        // Continue to process more requests
                    }
                    Poll::Ready(None) => {
                        // Request channel closed - exit
                        break;
                    }
                    Poll::Pending => {
                        // No more requests available
                        break;
                    }
                }
            }

            // If we processed requests, continue the loop to process more
            if processed_any {
                continue;
            }

            // Receive incoming messages (blocking)
            match transport.receive().await {
                Ok(server_msg) => {
                    let id = server_msg.id;

                    // Check if this is a response to a pending request
                    if let Some(tx) = receiver_state.pending_requests.remove(&id) {
                        // Send to pending request
                        let _ = tx.send(Ok(server_msg));
                    } else {
                        // Non-response message (log, etc.) - send to queue
                        // Check if it's a Log message or has ID 0 (server-initiated)
                        let is_non_response =
                            matches!(server_msg.msg, lp_model::server::ServerMsgBody::Log { .. })
                                || id == 0;

                        if is_non_response {
                            let _ = receiver_state.non_response_tx.send(server_msg);
                        }
                        // Orphaned response (no matching request) - ignore
                    }
                }
                Err(TransportError::ConnectionLost) => {
                    // Connection closed - send errors to all pending requests
                    for (_, tx) in receiver_state.pending_requests.drain() {
                        let _ = tx.send(Err(TransportError::ConnectionLost));
                    }
                    break;
                }
                Err(e) => {
                    // Other transport error - send to all pending requests
                    for (_, tx) in receiver_state.pending_requests.drain() {
                        let _ = tx.send(Err(e.clone()));
                    }
                    break;
                }
            }
        }

        // Close transport
        let _ = transport.close().await;
    }

    /// Send a request and wait for the response
    ///
    /// Creates a request message with a unique ID, sends it via the transport,
    /// and waits for the corresponding response.
    ///
    /// # Arguments
    ///
    /// * `request` - The client request to send
    ///
    /// # Returns
    ///
    /// * `Ok(ServerMessage)` if the response was received
    /// * `Err(ClientError)` if sending failed, the request timed out, or transport error
    pub async fn send_request(&self, request: ClientRequest) -> Result<ServerMessage, ClientError> {
        // Check if closed
        if self.closed.load(Ordering::Relaxed) {
            return Err(ClientError::Transport(TransportError::ConnectionLost));
        }

        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);

        // Create oneshot channel for response
        let (tx, mut rx) = oneshot::oneshot();

        // Create client message
        let client_msg = ClientMessage { id, msg: request };

        // Send request to receiver task (which will send via transport)
        self.request_tx
            .send((client_msg, tx))
            .map_err(|_| ClientError::Transport(TransportError::ConnectionLost))?;

        // Wait for response
        rx.await.map_err(|_| ClientError::Other {
            message: format!("Request {} was cancelled", id),
        })?
    }

    /// Get a mutable reference to the non-response message receiver
    ///
    /// Returns a receiver that can be polled for server-initiated messages
    /// like log messages. These messages don't correspond to any request.
    ///
    /// # Returns
    ///
    /// * `&mut UnboundedReceiver<ServerMessage>` - Receiver for non-response messages
    pub fn non_response_messages(&mut self) -> &mut UnboundedReceiver<ServerMessage> {
        &mut self.non_response_rx
    }

    /// Receive the next non-response message (blocking)
    ///
    /// Waits for the next non-response message (like log messages) to arrive.
    ///
    /// # Returns
    ///
    /// * `Ok(ServerMessage)` if a message was received
    /// * `Err(ClientError)` if receiving failed
    pub async fn receive_non_response(&mut self) -> Result<ServerMessage, ClientError> {
        use core::pin::Pin;
        use core::task::{Context, Poll};

        struct ReceiverFuture<'a, T>(Pin<&'a mut UnboundedReceiver<T>>);
        impl<'a, T> core::future::Future for ReceiverFuture<'a, T> {
            type Output = Option<T>;
            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                self.0.as_mut().poll_next(cx)
            }
        }

        ReceiverFuture(Pin::new(&mut self.non_response_rx))
            .await
            .ok_or_else(|| ClientError::Other {
                message: "Non-response message channel closed".to_string(),
            })
    }

    /// Close the client and stop the receiver task
    ///
    /// Closes the transport and stops the background receiver task.
    /// Any pending requests will receive connection lost errors.
    pub async fn close(self) -> Result<(), ClientError> {
        // Mark as closed
        self.closed.store(true, Ordering::Relaxed);

        // The receiver task will handle closing the transport and cancelling pending requests
        Ok(())
    }
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    use super::*;
    use crate::transport::local::LocalTransport;
    use alloc::string::ToString;
    use lp_model::server::{FsResponse, api::LogLevel};
    use lp_shared::transport::ServerTransport;

    /// Fake server that processes requests and sends responses
    struct FakeServer {
        transport: LocalTransport,
    }

    impl FakeServer {
        fn new(transport: LocalTransport) -> Self {
            Self { transport }
        }

        /// Process one request and send response
        fn process_one_request(&mut self) -> Result<bool, TransportError> {
            match ServerTransport::receive(&mut self.transport)? {
                Some(client_msg) => {
                    // Process the request and send a response
                    let response = match &client_msg.msg {
                        ClientRequest::Filesystem(lp_model::server::FsRequest::Read { path }) => {
                            // Echo back the path with some fake data
                            ServerMessage {
                                id: client_msg.id,
                                msg: lp_model::server::ServerMsgBody::Filesystem(
                                    FsResponse::Read {
                                        path: path.clone(),
                                        data: Some(b"fake content".to_vec()),
                                        error: None,
                                    },
                                ),
                            }
                        }
                        ClientRequest::Filesystem(lp_model::server::FsRequest::Write {
                            path,
                            data: _,
                        }) => ServerMessage {
                            id: client_msg.id,
                            msg: lp_model::server::ServerMsgBody::Filesystem(FsResponse::Write {
                                path: path.clone(),
                                error: None,
                            }),
                        },
                        _ => {
                            // For other requests, send an error response
                            ServerMessage {
                                id: client_msg.id,
                                msg: lp_model::server::ServerMsgBody::Filesystem(
                                    FsResponse::Read {
                                        path: "unknown".to_string(),
                                        data: None,
                                        error: Some("Unsupported request".to_string()),
                                    },
                                ),
                            }
                        }
                    };
                    ServerTransport::send(&mut self.transport, response)?;
                    Ok(true)
                }
                None => Ok(false),
            }
        }

        /// Send a log message (non-response message)
        fn send_log(&mut self, level: LogLevel, message: String) -> Result<(), TransportError> {
            let log_msg = ServerMessage {
                id: 0, // ID 0 indicates server-initiated message
                msg: lp_model::server::ServerMsgBody::Log { level, message },
            };
            ServerTransport::send(&mut self.transport, log_msg)
        }
    }

    /// Set up client and server with LocalTransport
    fn setup() -> (LpClient, ReceiverTaskComponents, FakeServer) {
        let (client_transport, server_transport) = LocalTransport::new_pair();
        let (client, components) = LpClient::new(Box::new(client_transport));
        let server = FakeServer::new(server_transport);
        (client, components, server)
    }

    #[tokio::test]
    async fn test_send_request_and_receive_response() {
        // Arrange
        let (mut client, components, mut server) = setup();
        tokio::spawn(LpClient::receiver_task(components));

        // Act: Send a read request
        let request = ClientRequest::Filesystem(lp_model::server::FsRequest::Read {
            path: "/test.txt".to_string(),
        });
        let response_fut = client.send_request(request);

        // Process the request on the server side
        tokio::task::yield_now().await; // Yield to allow receiver task to send
        server.process_one_request().unwrap();

        // Assert: Receive the response
        let response = response_fut.await.unwrap();
        match response.msg {
            lp_model::server::ServerMsgBody::Filesystem(FsResponse::Read { path, data, error }) => {
                assert_eq!(path, "/test.txt");
                assert_eq!(data, Some(b"fake content".to_vec()));
                assert_eq!(error, None);
            }
            _ => panic!("Unexpected response type"),
        }
    }

    #[tokio::test]
    async fn test_multiple_concurrent_requests() {
        // Arrange
        let (mut client, components, mut server) = setup();
        tokio::spawn(LpClient::receiver_task(components));

        // Act: Send multiple requests
        let request1 = ClientRequest::Filesystem(lp_model::server::FsRequest::Read {
            path: "/file1.txt".to_string(),
        });
        let request2 = ClientRequest::Filesystem(lp_model::server::FsRequest::Read {
            path: "/file2.txt".to_string(),
        });
        let request3 = ClientRequest::Filesystem(lp_model::server::FsRequest::Write {
            path: "/file3.txt".to_string(),
            data: b"content".to_vec(),
        });

        let response1_fut = client.send_request(request1);
        let response2_fut = client.send_request(request2);
        let response3_fut = client.send_request(request3);

        // Process all requests on the server side
        tokio::task::yield_now().await; // Yield to allow receiver task to send
        for _ in 0..3 {
            server.process_one_request().unwrap();
        }

        // Assert: All responses should be received
        let response1 = response1_fut.await.unwrap();
        let response2 = response2_fut.await.unwrap();
        let response3 = response3_fut.await.unwrap();

        match response1.msg {
            lp_model::server::ServerMsgBody::Filesystem(FsResponse::Read { path, .. }) => {
                assert_eq!(path, "/file1.txt");
            }
            _ => panic!("Unexpected response type"),
        }

        match response2.msg {
            lp_model::server::ServerMsgBody::Filesystem(FsResponse::Read { path, .. }) => {
                assert_eq!(path, "/file2.txt");
            }
            _ => panic!("Unexpected response type"),
        }

        match response3.msg {
            lp_model::server::ServerMsgBody::Filesystem(FsResponse::Write { path, .. }) => {
                assert_eq!(path, "/file3.txt");
            }
            _ => panic!("Unexpected response type"),
        }
    }

    #[tokio::test]
    async fn test_receive_non_response_message() {
        // Arrange
        let (mut client, components, mut server) = setup();
        tokio::spawn(LpClient::receiver_task(components));

        // Act: Server sends a log message
        server
            .send_log(LogLevel::Info, "Test log message".to_string())
            .unwrap();

        // Yield to allow receiver task to process
        tokio::task::yield_now().await;

        // Assert: Client receives the log message
        let log_msg = client.receive_non_response().await.unwrap();
        match log_msg.msg {
            lp_model::server::ServerMsgBody::Log { level, message } => {
                assert_eq!(level, LogLevel::Info);
                assert_eq!(message, "Test log message");
            }
            _ => panic!("Expected log message"),
        }
    }

    #[tokio::test]
    async fn test_request_id_correlation() {
        // Arrange
        let (mut client, components, mut server) = setup();
        let receiver_handle = tokio::spawn(LpClient::receiver_task(components));

        // Act: Send two requests
        let request1 = ClientRequest::Filesystem(lp_model::server::FsRequest::Read {
            path: "/file1.txt".to_string(),
        });
        let request2 = ClientRequest::Filesystem(lp_model::server::FsRequest::Read {
            path: "/file2.txt".to_string(),
        });

        let response1_fut = client.send_request(request1);
        let response2_fut = client.send_request(request2);

        // Give receiver task time to send requests to transport
        tokio::task::yield_now().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Process requests on server side - receiver task should have sent them
        let mut processed_count = 0;
        for _ in 0..20 {
            if server.process_one_request().unwrap() {
                processed_count += 1;
                if processed_count == 2 {
                    break;
                }
            }
            tokio::task::yield_now().await;
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        assert_eq!(processed_count, 2, "Both requests should be processed");

        // Give receiver task time to receive responses
        tokio::task::yield_now().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Assert: Responses match their requests by ID
        // Use tokio::time::timeout to avoid hanging if something is wrong
        let response1 = tokio::time::timeout(tokio::time::Duration::from_secs(1), response1_fut)
            .await
            .expect("Response 1 should arrive within 1 second")
            .unwrap();

        let response2 = tokio::time::timeout(tokio::time::Duration::from_secs(1), response2_fut)
            .await
            .expect("Response 2 should arrive within 1 second")
            .unwrap();

        // First request should have ID 1, second should have ID 2
        assert_eq!(response1.id, 1, "First response should have ID 1");
        assert_eq!(response2.id, 2, "Second response should have ID 2");

        // Verify the responses match their requests
        match response1.msg {
            lp_model::server::ServerMsgBody::Filesystem(FsResponse::Read { path, .. }) => {
                assert_eq!(path, "/file1.txt", "Response 1 should match request 1 path");
            }
            _ => panic!("Unexpected response type for response 1"),
        }

        match response2.msg {
            lp_model::server::ServerMsgBody::Filesystem(FsResponse::Read { path, .. }) => {
                assert_eq!(path, "/file2.txt", "Response 2 should match request 2 path");
            }
            _ => panic!("Unexpected response type for response 2"),
        }

        // Clean up - close client to stop receiver task
        drop(client);
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        receiver_handle.abort();
    }

    #[tokio::test]
    async fn test_close_client() {
        // Arrange
        let (client, components, _server) = setup();
        tokio::spawn(LpClient::receiver_task(components));

        // Act: Close the client
        let result = client.close().await;

        // Assert: Close succeeds
        assert!(result.is_ok());

        // Give receiver task time to process close
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
}
