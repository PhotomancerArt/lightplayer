# Notes: LP Client-Server Sync

## Unanswered Questions

### Request/Response Correlation

**DECIDED: Request IDs with message envelope**

**Decision:**
- **Request IDs** - Client-generated unique IDs for each request
- **Message envelope** - Top-level message enum that wraps requests/responses/logs with request IDs
- Structure:
  - `Message` enum with variants: `Request { id: u64, request: ServerRequest }`, `Response { id: u64, response: ServerResponse }`, `Log { ... }`, etc.
  - Client generates request ID, wraps request in envelope
  - Server wraps response with same ID
  - Allows interleaved messages (logs can come between request and response)
  - Supports async requests (multiple in flight)

### Server-Sent Messages

**DECIDED: Polling approach with message buffer**

**Decision:**
- **Polling approach** - `ClientTransport` has `receive_message() -> Result<Message>` method
- Client polls transport for messages (requests, responses, server-sent)
- **Message buffer** - Transport can buffer messages internally
  - Clients generally have lots of memory, so storing messages shouldn't be an issue
  - May want a limit on buffer size (to be determined)
- All messages (requests, responses, logs, etc.) go through same `Message` enum
- Client handles messages as they poll

### Other Open Questions

**Serialization Format:**

**DECIDED: JSON with compression**

**Decision:**
- **JSON** - Use JSON for message serialization (simplicity, compatibility, debugging)
- **Compression** - Apply compression (gzip/zlib) to reduce payload size, especially for textures
  - Textures are ~10-20KB for 64x64, which is large for JSON
  - Compression will help significantly
  - Concern: binary size (need to check if we have room for compression library)
- Format is fixed (not configurable per transport) for initial implementation
- Message versioning: Not needed initially, can add later if protocol evolves

**Error Handling:**
- How do we handle transport errors? (connection lost, timeout, etc.)
- Should client retry automatically?
- How do we distinguish transport errors from protocol errors?

**Connection Management:**
- How do we handle reconnection?
- Should client automatically reconnect?
- What happens to in-flight requests on disconnect?

**Frame Synchronization:**

**DECIDED: No special handling needed**

**Decision:**
- **Full resync** - Just use `GetChanges` with `since_frame = 0` (returns all nodes)
- **Incremental sync** - Use `GetChanges` with `since_frame = last_frame_id`
- No special detection or handling needed - client can choose when to do full vs incremental
- Server always handles both cases the same way

**Authentication/Authorization:**
- Do we need authentication?
- How do we handle unauthorized access?
- Is this needed for initial implementation?

**Rate Limiting:**
- Should server rate limit requests?
- Should client throttle requests?
- Is this needed for initial implementation?
