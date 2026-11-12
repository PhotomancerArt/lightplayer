# Phase 4: Create AsyncClientTransport

## Description

Create `AsyncClientTransport` struct that wraps a sync `ClientTransport`, spawns a background polling task, and provides async request/response correlation via channels. This enables multiple async consumers to share a single transport without mutex contention.

## Tasks

1. Create `lp-app/apps/lp-cli/src/client/async_transport.rs`:
   - Define `AsyncClientTransport` struct with:
     - `request_tx: mpsc::UnboundedSender<(ClientMessage, oneshot::Sender<Result<ServerMessage, TransportError>>)>`
     - `error_rx: mpsc::Receiver<TransportError>`
     - `poller_handle: JoinHandle<()>`
     - `closed: Arc<AtomicBool>`
   - Implement `new(transport: Box<dyn ClientTransport + Send>) -> Self`:
     - Create channels for requests and errors
     - Spawn background polling task that:
       - Owns the transport
       - Polls `transport.receive_all()` in a loop
       - Matches responses to pending requests by ID
       - Sends responses via oneshot channels
       - Sends errors to error channel
       - Yields frequently (`tokio::task::yield_now()`)
       - Exits when `closed` flag is set
     - Return struct with channels and handle
   - Implement `async fn send_request(&self, msg: ClientMessage) -> Result<ServerMessage, TransportError>`:
     - Create oneshot channel
     - Send `(msg, oneshot_tx)` via `request_tx`
     - Wait on `oneshot_rx` for response
     - Return response or error
   - Implement `fn error_rx(&self) -> &mpsc::Receiver<TransportError>`
   - Implement `async fn close(&mut self) -> Result<(), TransportError>`:
     - Set `closed` flag
     - Close `request_tx` channel (signals poller to stop)
     - Wait for poller handle to finish
     - Close underlying transport if possible
   - Implement `Drop` that calls `close()` if not already called (best-effort)

2. Update `lp-app/apps/lp-cli/src/client/mod.rs`:
   - Add `pub mod async_transport;`
   - Re-export `AsyncClientTransport`

3. Add tests:
   - Test with mock `ClientTransport` (use `LocalTransport` from lp-shared)
   - Test `send_request()` waits for response
   - Test request/response correlation (multiple concurrent requests)
   - Test error channel receives transport errors
   - Test `close()` stops polling task
   - Test `Drop` calls `close()` if not already called

## Success Criteria

- `AsyncClientTransport` struct exists and compiles
- Background polling task spawns and runs correctly
- `send_request()` waits for response and returns it
- Multiple concurrent requests work correctly
- Error channel receives transport errors
- `close()` stops polling task
- Tests pass
- Code compiles without errors

## Implementation Notes

- Background task should use `tokio::spawn()` for async runtime
- Use `HashMap<u64, oneshot::Sender<...>>` to track pending requests
- Extract request ID from `ClientMessage::id` field
- Match response ID from `ServerMessage::id` field
- Use `tokio::task::yield_now().await` frequently in polling loop
- Use `tokio::time::sleep(Duration::from_millis(1))` for small delays
- Error channel should be bounded (e.g., `mpsc::channel(100)`) to prevent unbounded growth
- Consider timeout for `send_request()` (e.g., 30 seconds) to prevent hanging
