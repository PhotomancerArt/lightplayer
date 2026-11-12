# Plan Notes: Transport Refactor

## Context

The transport layer currently uses a `Message` wrapper type (`Vec<u8>`) that requires callers to handle serialization/deserialization. This is incorrect - the transport should handle serialization internally and work directly with `ClientMessage` and `ServerMessage` from `lp-model`.

Additionally, `MemoryTransport` is currently in `lp-client` but should be in `lp-shared` behind a feature gate. It should be refactored to be explicitly single-threaded (no std requirement).

## Questions

### Q1: Single-threaded memory transport name

**Context:**
The current `MemoryTransport` uses `Arc<Mutex>` for thread-safety when `std` is enabled, and `Rc<RefCell>` for `no_std`. We want to refactor it to be explicitly single-threaded, removing the need for `std`.

**Suggested answer:**
`LocalMemoryTransport` - clearly indicates it's for local/single-threaded use cases.

**Alternatives considered:**
- `SingleThreadMemoryTransport` - too verbose
- `SyncMemoryTransport` - ambiguous (could mean synchronous vs async)
- `DirectMemoryTransport` - doesn't convey single-threaded nature

**Answer:** `LocalMemoryTransport` ✓

### Q2: Transport trait location

**Context:**
The transport traits (`ClientTransport`, `ServerTransport`) are currently in `lp-shared`. They need to depend on `lp-model` to use `ClientMessage` and `ServerMessage`.

**Question:**
Should the transport traits stay in `lp-shared` (with `lp-model` as a dependency), or move to `lp-model`?

**Suggested answer:**
Keep in `lp-shared` with `lp-model` dependency. This maintains separation of concerns - `lp-shared` contains shared infrastructure, and `lp-model` contains the message protocol types.

**Answer:** Keep in `lp-shared` with `lp-model` dependency ✓

### Q3: Feature gate name for memory transport

**Context:**
The memory transport will be in `lp-shared` behind a feature gate.

**Question:**
What should the feature gate be named?

**Suggested answer:**
`transport-memory` - follows the pattern of `transport-*` for transport implementations.

**Answer:** No feature gate needed for `LocalMemoryTransport` - it's `no_std` compatible. Only transports that require `std` features need gates. ✓

### Q4: Serialization format

**Context:**
The memory transport currently uses JSON serialization. Other transports (stdio, websocket, etc.) may use different formats.

**Question:**
Should the transport trait specify the serialization format, or should each transport implementation choose its own format?

**Suggested answer:**
Each transport implementation chooses its own format. The trait doesn't need to specify format - it just needs to handle serialization/deserialization internally. This allows for JSON (memory), binary (future), or other formats.

**Answer:** Transport handles serialization internally, each transport can choose its own format ✓

### Q5: Error handling for serialization failures

**Context:**
Serialization/deserialization errors need to be handled by the transport implementations.

**Question:**
Should serialization errors be wrapped in `TransportError::Serialization` and `TransportError::Deserialization`, or should we add more specific error variants?

**Suggested answer:**
Use existing `TransportError::Serialization` and `TransportError::Deserialization` variants. They already accept `String` messages, which is sufficient for now.

**Answer:** Use existing error variants ✓

### Q6: Backward compatibility

**Context:**
Existing code uses the transport traits with the `Message` wrapper type.

**Question:**
Do we need to maintain backward compatibility, or can we break the API?

**Suggested answer:**
Break the API - this is a refactoring to fix an architectural issue. All callers will need to be updated to use the new API.

**Answer:** Break the API - we're in active development ✓

### Q7: Update scope

**Context:**
The refactor will affect:
- Transport trait definitions
- Memory transport implementation
- All code that uses transports (client, server, tests)

**Question:**
Should this plan include updating all callers, or just the transport layer itself?

**Suggested answer:**
Include updating all callers in the plan. This ensures the refactor is complete and tests pass.

**Answer:** Update all callers, ensure tests pass in lp-app ✓

## Notes
