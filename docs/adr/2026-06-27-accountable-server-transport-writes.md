# ADR: Accountable Server Transport Writes

- **Status:** Accepted
- **Date:** 2026-06-27
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Project reads are now sent as ordered `ProjectReadFrame` messages. The frame
sequence is an integrity check: if the client receives frame `4` while waiting
for frame `3`, the stream is corrupt and must fail.

On ESP32, server responses previously moved through a one-way outgoing message
channel owned by the serial I/O task. `ServerTransport::send().await` could
return after that channel handoff even though the USB write had not happened
yet. If the device disconnected, serialization overflowed, or USB timed out,
the I/O task could drop a server frame after the project-read sender had
already advanced to the next sequence number. The user-visible symptom was a
protocol error such as "expected project read frame 3, got 4".

Increasing buffers would only move the failure boundary and spend scarce
firmware RAM. The transport contract needed to make write failure observable at
the point where frame ordering is decided.

## Decision

`ServerTransport::send(message).await` means the message has been accepted by
the underlying transport write path, or an error has been returned. A transport
must not report success for a best-effort handoff that can later drop the
message without surfacing that failure to the awaiting future.

ESP32 keeps USB ownership in `io_task`. That task already owns
`UsbSerialJtag::into_async().split()`, inbound reads, log writes, and connection
monitoring. Instead of moving USB TX ownership into the server loop, the ESP32
server transport now submits one accountable write request to `io_task` and
awaits the matching write result.

Project-read frame sending advances its sequence only after `send().await`
returns success. If a frame write fails, the stream returns that error and keeps
the failed frame pending instead of emitting later frames.

## Consequences

- Silent skipped project-read frames are no longer an allowed transport state.
- ESP32 has a single in-flight server response write, preserving backpressure.
- Disconnected USB, serialization overflow, and USB write timeout are surfaced
  to the server loop as transport errors.
- `io_task` remains the single ESP32 USB owner, avoiding a broader ownership
  rewrite.
- Project-read streams require an explicit final flush so the last partial
  frame is written even when it did not fill the frame budget.
- The 16 KiB project-read frame budget remains the wire-size target; firmware
  scratch buffers may include only small framing/serializer margin.

## Alternatives Considered

- **Larger firmware buffers:** simple, but consumes RAM and leaves the same
  skipped-frame failure mode when a future payload crosses the new limit.
- **Direct server-loop USB writes:** makes acknowledgement obvious, but fights
  the existing ESP32 ownership model where `io_task` also handles reads, logs,
  and connection monitoring.
- **Async giant JSON streaming:** helps one serialization queue, but still
  asks transports such as WebSocket or TCP to handle a large logical message.
- **Binary protocol:** likely useful later for payload efficiency, but it does
  not replace the need for accountable write completion.

## Follow-ups

- Hardware-verify that old firmware or disconnected USB now fails as a clean
  transport error without producing skipped project-read sequences.
- Consider request-scoped completion tokens if ESP32 ever allows more than one
  server response write in flight.
- Revisit binary payload encoding after the bounded-message and accountable
  write contracts have settled.
