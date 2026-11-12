# Phase 6: Implement LpClient with Blocking Operations

## Goal

Create `LpClient` struct with blocking filesystem operations that handle request/response correlation.

## Tasks

1. Create `lp-client/src/error.rs`:
   - Define `ClientError` enum:
     - `Transport(TransportError)`
     - `Timeout { request_id: u64 }`
     - `Protocol { message: String }`
     - `Other { message: String }`
   - Implement `Display` trait

2. Create `lp-client/src/client.rs`:
   - Define `PendingRequest` struct:
     ```rust
     struct PendingRequest {
         request_type: String, // for debugging
         response_tx: Option<ServerResponse>, // or use channel
     }
     ```
   - Or use simpler approach: store expected response type
   - Define `LpClient` struct:
     ```rust
     pub struct LpClient<T: ClientTransport> {
         transport: T,
         next_request_id: u64,
         pending_requests: HashMap<u64, PendingRequest>,
     }
     ```
   - Implement `new()`:
     ```rust
     pub fn new(transport: T) -> Self
     ```
   - Implement `process_messages()`:
     - Poll `transport.receive()`
     - Match responses to pending requests by ID
     - Store responses for waiting operations
   - Implement `send_request()`:
     - Generate request ID
     - Create `ClientMessage` with ID
     - Send via transport
     - Poll for response (with timeout)
     - Return response or error
   - Implement filesystem operations:
     - `fs_read()`: Create `FsRequest::Read`, send, wait for response
     - `fs_write()`: Create `FsRequest::Write`, send, wait for response
     - `fs_delete_file()`: Create `FsRequest::DeleteFile`, send, wait for response
     - `fs_delete_dir()`: Create `FsRequest::DeleteDir`, send, wait for response
     - `fs_list_dir()`: Create `FsRequest::ListDir`, send, wait for response
   - All operations are blocking (poll internally)

3. Update `lp-client/src/lib.rs`:
   - Export `client` module
   - Export `LpClient` struct
   - Export `error` module

4. Add timeout mechanism:
   - Simple: max iterations (e.g., 1000 polls)
   - Or time-based if `std` feature available
   - Return `ClientError::Timeout` if timeout exceeded

5. Handle errors from responses:
   - Check `error` field in `FsResponse` variants
   - Convert to `ClientError::Protocol` or appropriate error

## Success Criteria

- `LpClient` struct exists with filesystem operation methods
- Request/response correlation works correctly
- Operations are blocking (poll internally)
- Timeout mechanism works
- Error handling works correctly
- All code compiles without warnings
