# Research: Message Queue/Routing Solutions for no_std Async

## Summary

**Recommended Solution: `embassy-sync::channel::Channel`**

Embassy-sync provides exactly what we need: a no_std, no-alloc, async-compatible MPMC channel that's already in our dependency tree.

## Embassy-Sync Channel

### Key Features
- **no_std compatible**: Works in embedded environments
- **no-alloc**: Uses const generics for compile-time size (`Channel<M, T, const N: usize>`)
- **MPMC**: Multiple Producer Multiple Consumer - perfect for multi-transport future
- **Bounded**: Configurable capacity (prevents unbounded growth)
- **Async-compatible**: Returns futures for `send()`/`receive()`
- **Non-blocking**: `try_send()`/`try_receive()` for polling
- **Cross-platform**: Works with different mutex types (CriticalSectionRawMutex, ThreadModeMutex, etc.)

### API Overview
```rust
use embassy_sync::channel::Channel;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

// Static channel (compile-time size)
static INCOMING_QUEUE: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();

// Async send (waits if full)
INCOMING_QUEUE.send(message).await;

// Non-blocking send (returns error if full)
INCOMING_QUEUE.try_send(message)?;

// Async receive (waits if empty)
let msg = INCOMING_QUEUE.receive().await;

// Non-blocking receive (returns error if empty)
let msg = INCOMING_QUEUE.try_receive()?;

// Get sender/receiver handles
let sender = INCOMING_QUEUE.sender();
let receiver = INCOMING_QUEUE.receiver();
```

### Advantages
1. **Already in dependency tree**: `embassy-executor` depends on `embassy-sync`
2. **Production-ready**: Used widely in embedded Rust
3. **Perfect fit**: Designed for exactly our use case (async tasks, no_std, bounded queues)
4. **Multi-transport ready**: MPMC means multiple transports can send/receive
5. **Backpressure handling**: Bounded capacity prevents memory issues
6. **Cross-platform**: Works on ESP32, RISC-V, and other embedded targets

### Overflow Strategy
- `try_send()` returns `TrySendError::Full(T)` if channel is full
- We can implement "drop oldest" by:
  1. Try send, if full:
  2. Receive oldest message (non-blocking)
  3. Try send again
  4. Track dropped count

## Alternative Solutions Considered

### 1. Custom MessageQueue<T>
**Pros:**
- Full control over API
- Can implement exact overflow strategy we want

**Cons:**
- Reinventing the wheel
- More code to maintain
- Need to handle thread-safety ourselves
- Less battle-tested

**Verdict:** Not recommended - embassy-sync already provides what we need

### 2. bbqueue
**Pros:**
- SPSC (single-producer, single-consumer) lockless queue
- no_std compatible

**Cons:**
- SPSC only (not MPMC) - doesn't fit multi-transport future
- Would need multiple queues
- Less integrated with embassy ecosystem

**Verdict:** Not suitable for our multi-transport architecture

### 3. crossbeam-queue
**Pros:**
- MPMC queues
- Well-tested

**Cons:**
- Requires `alloc` feature for no_std
- Less integrated with embassy
- More dependencies

**Verdict:** Could work but embassy-sync is better integrated

### 4. tokio::sync::mpsc
**Pros:**
- Used in our std codebase (lp-cli, lp-client)
- Well-tested

**Cons:**
- Requires std (tokio runtime)
- Not no_std compatible
- Can't use in firmware

**Verdict:** Only for host-side code, not firmware

## Architecture Recommendation

### For Firmware (no_std)
Use `embassy-sync::channel::Channel`:
- Incoming queue: `Channel<CriticalSectionRawMutex, String, 32>` (for test)
- Outgoing queue: `Channel<CriticalSectionRawMutex, String, 32>` (for test)
- Future: `Channel<CriticalSectionRawMutex, ClientMessage, 64>` (for real code)

### For Host Tests (std)
Can use `tokio::sync::mpsc` for host-side test automation, but firmware uses embassy-sync.

### Message Router Pattern
```rust
// In fw-core
pub struct MessageRouter {
    incoming: &'static Channel<CriticalSectionRawMutex, String, 32>,
    outgoing: &'static Channel<CriticalSectionRawMutex, String, 32>,
}

impl MessageRouter {
    // Main loop calls this
    pub fn receive_all(&self) -> Vec<String> {
        let mut messages = Vec::new();
        let receiver = self.incoming.receiver();
        while let Ok(msg) = receiver.try_receive() {
            messages.push(msg);
        }
        messages
    }
    
    // Main loop calls this
    pub fn send(&self, msg: String) -> Result<(), TrySendError<String>> {
        self.outgoing.try_send(msg)
    }
}

// I/O task drains queues
async fn io_task(router: MessageRouter) {
    let incoming_receiver = router.incoming.receiver();
    let outgoing_sender = router.outgoing.sender();
    
    loop {
        // Drain outgoing queue and send via serial
        while let Ok(msg) = outgoing_sender.try_receive() {
            // Send via serial (handle errors gracefully)
        }
        
        // Read from serial and push to incoming queue
        // (handle serial errors gracefully)
        
        embassy_time::Timer::after(Duration::from_millis(1)).await;
    }
}
```

## Decision

**Use `embassy-sync::channel::Channel` for firmware message queues.**

This provides:
- ✅ no_std compatibility
- ✅ Async support
- ✅ Bounded queues (backpressure)
- ✅ MPMC (multi-transport ready)
- ✅ Already in dependencies
- ✅ Cross-platform (works on ESP32, RISC-V, etc.)
- ✅ Production-ready and battle-tested
