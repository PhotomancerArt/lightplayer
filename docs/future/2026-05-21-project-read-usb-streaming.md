# Project-read JSON is not end-to-end streamed to USB

## Status: future transport fix

Captured 2026-05-21 after initial debug UI shape sync failed on ESP32-C6 with:

```text
Serial thread: Failed to parse JSON message: EOF while parsing a string at line 1 column 16382
run_server_loop: Server tick error: Core("Serialization error: project-read JSON write failed")
```

The cutoff near 16 KiB was not a host receive-buffer limit. It was the firmware
server filling its bounded outgoing JSON chunk queue while serializing a
`project-read` response.

## What is actually streaming today

The project-read response does use streaming JSON serialization. The engine does
not build one full `ProjectReadResponse` or one giant JSON `String` before it
starts writing bytes.

The current firmware path is still not end-to-end streaming:

```text
Engine project-read source
        │
        ▼
JsonWriter / SlotWriter
        │
        ▼
ChunkedJsonWriter
        │  1 KiB chunks, synchronous JsonWrite::write_all
        ▼
OUTGOING_SERVER_JSON_CHUNK channel
        │  fixed capacity, currently 16 chunks
        ▼
io_task
        │  async timed_write_all
        ▼
USB serial
```

The important boundary is `ChunkedJsonWriter -> OUTGOING_SERVER_JSON_CHUNK`.
`JsonWrite::write_all` is synchronous, and the writer uses a non-blocking queue
send. While the server task is inside the synchronous serialization call, it
does not yield to the async executor, so `io_task` does not get scheduled to
drain the queue. As a result, the queue must hold the whole serialized frame
up to the next yield point.

That means the implementation is:

- streaming from engine data structures into JSON bytes,
- chunking those bytes,
- but still requiring a bounded in-RAM frame budget before USB writes can catch
  up.

This is why increasing the queue capacity appears to fix large shape sync, but
does so by spending static ESP RAM. That is not an acceptable product fix.

## Why not increase the buffer

The queue is static firmware RAM. Increasing it from 16 KiB to 64 KiB made the
ESP32-C6 build fail at link time with `.bss` overflowing RAM. Even smaller
increases are the wrong direction: the server transport should not require RAM
proportional to a debug UI shape snapshot.

The on-device compiler and runtime are the product. Serial sync must fit around
their RAM needs, not consume large static buffers to paper over backpressure.

## Near-term mitigations

### Byte-budget shape pages

The debug UI already requests shape pages with `ShapeReadQuery { limit: Some(1)
}` during initial sync. The failing response shows that one shape registry entry
plus envelope can exceed the current queue budget.

A safer short-term protocol fix is to make shape sync byte-budgeted rather than
count-budgeted. The server should be able to return a partial shape entry or a
smaller representation when a single shape would exceed the transport frame
budget.

Open design questions:

- Should large shape entries be split into subdocuments, or should large shapes
  be refactored into registered shape refs?
- Should `ShapeReadQuery` grow a `max_bytes` hint, with an explicit
  `TooLarge`/`NeedsSplit` result when a single item cannot fit?
- Should the client treat shape sync as a separate protocol from general
  `project-read`, with smaller frames and stricter budgets?

### Reduce large shape entries

If a single shape entry is huge, it may indicate the shape registry is embedding
large nested shapes repeatedly instead of using `SlotShape::Ref`. Reducing shape
entry size would help every transport and should be investigated independently
of the USB streaming fix.

## Real fix: stream project-read directly to USB

The firmware transport needs a project-read path whose backpressure reaches the
USB writer instead of stopping at an in-RAM channel.

The intended shape is:

```text
send_project_read(...).await
        │
        ▼
async JSON writer with a small stack/static chunk
        │
        ▼
timed_write_all(tx, chunk).await
        │
        ▼
continue serialization after USB accepts bytes
```

There are two plausible implementation routes.

### Option A: async project-read JSON writer

Add an async firmware-only JSON sink and async project-read serialization entry
points. The writer would buffer a small chunk, flush it to USB with
`timed_write_all(...).await`, then continue.

Pros:

- true end-to-end streaming;
- tiny bounded RAM footprint;
- straightforward backpressure semantics.

Cons:

- requires async variants of the current project-read writer stack;
- async traits or explicit generic functions may add some type complexity;
- cannot simply reuse `serde::Serialize` through the current synchronous
  `JsonWriter::serde` bridge without a synchronous buffer boundary.

### Option B: resumable project-read frame generator

Keep JSON writing synchronous, but serialize only up to a small byte budget,
return control to the transport, write that chunk to USB, then resume from an
explicit cursor/state machine.

Pros:

- avoids async writer traits;
- makes frame budgeting explicit.

Cons:

- much more manual state;
- easy to make correctness bugs in JSON comma/object state;
- likely duplicates logic already present in `JsonWriter`.

## Recommendation

Do not increase `OUTGOING_SERVER_JSON_CHUNK` capacity beyond the current small
budget.

Short term:

- keep the queue small;
- add byte-budgeted shape sync or shrink oversized shape entries so initial UI
  sync fits the existing budget;
- improve diagnostics so failures name the exhausted frame budget instead of
  saying only "project-read JSON write failed".

Long term:

- implement a firmware project-read transport path that writes directly to USB
  with async backpressure;
- keep the host/websocket transports on the existing simpler path unless they
  need the same memory behavior.

The current JSON writer work is still useful: it prevents allocating the full
semantic response. The missing piece is carrying that streaming property across
the firmware transport boundary all the way to USB.
