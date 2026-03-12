# Phases

1. Consolidate uncommitted changes (keep useful lp-model, discard chunked)
2. Update ServerTransport trait to async + all transports + server loops
3. ESP32: SerWrite for serial, OUTGOING_SERVER_MSG, io_task serialize
4. ESP32: Rewrite StreamingMessageRouterTransport, switch main to use it
5. ESP32: Verify server_loop await (done in phase 2)
6. Verify other transports (SerialTransport, MessageRouterTransport, FakeTransport, WebSocket, AsyncLocal)
7. Verify fw-emu and CLI server loops
8. Adapt test_json
9. Cleanup and validation
