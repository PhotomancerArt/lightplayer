---
status: fixed
found: 2026-09-08      # how: hardware-walk (c6-bootloader-hang-walk.sh)
fixed: this change
area: lpa-client stream/serialport_stream + transport_serial (hardware framing thread, AsyncSerialClientTransport::close); lpa-link host_serial_esp32 provider + DeviceSession::release_link
class: lifecycle-ownership
related:
  - 2026-09-06-c6-analog-master-wedges-the-bootloader.md
  - 2026-09-04-pre-flash-hello-stamps-over-a-closed-port.md
  - ~/.photomancer/planning/lp2025/2026-09-06-1649-c6-first-flash-bootloader-hang/
---
# A serial close that a wedged device could veto

**Symptom** — On the C6 first-flash bench walk
(`scripts/c6-bootloader-hang-walk.sh <mac>`), against a board deliberately
hung in its bootloader: `DeviceSession::connect` correctly reported
`Unresponsive { NoSerialOutput }`, and the very next step —
`manage(EraseDeviceFlash)`, whose first act is `release_link()` → provider
`close()` → `ClientTransport::close().await` → reopen the same port by name
— failed to open it:

```
failed to open /dev/cu.usbmodem13101 after 20 attempts: Device or resource
busy
```

Twenty attempts is the two-second retry added in `host_esp32_flash::open_port`
precisely for the macOS "just-closed `/dev/cu.*` is busy for a beat" window.
The port was not busy for a beat; it was held for good. Nothing in the walk
log said so: `release_link()` discarded the close error with `let _ =`, so the
only visible failure was the reopen, one step downstream of the layer that had
actually failed.

## Root cause

Two layers both behaved as if they owned the port's lifetime, and neither
actually did.

The framing thread spawned by `create_hardware_serial_transport_pair_with_options`
is the **sole** owner of the `DeviceByteStream`, so the OS port fd is released
at exactly one instant: when that thread returns. `AsyncSerialClientTransport::close`
knows this and joins the thread — but with a one-second budget, after which it
gave up and returned `Err`. The thread kept running, kept the fd, and the
transport (still alive inside the `Arc<Mutex<..>>` the `DeviceWire` holds until
the post-management rebuild) had no other release path. "Closed" was a claim
the caller could not enforce.

What kept the thread past the budget is the device itself. Both of the loop's
blocking calls were supposed to be bounded, and one was not:

- `SerialPortByteStream::write_all` called `port.flush()`. In `serialport`
  4.9.0 that is `tcdrain(fd)` — *block until every queued byte has been
  transmitted* — and it takes **no timeout** (the port timeout there only
  bounds `EINTR` retries). A device that has stopped draining its receive
  FIFO — a C6 spinning in `rtc_clk_init` with its LP analog I2C clock gated is
  exactly that, see the sibling defect — backs the queue up and turns
  `tcdrain` into a permanent block, inside a thread whose shutdown signal is
  only ever read *between* calls.
- Even short of a wedge, a slow device compounded it: the message drain
  (`while let Ok(msg) = client_rx.try_recv()`) ran to the end of the queue
  before the loop looked at the shutdown signal again. Readiness re-asks a
  hello every second for the whole ready budget (30 s on the walk), so a
  device that never answers leaves a deep backlog, and a close landing
  mid-drain waited out every remaining write.

The `Unresponsive` path is what exposed it, and not by coincidence: a board
that reaches `Ready` drains its FIFO on every frame and queues nothing, so
`tcdrain` returns promptly and the drain loop is always empty at close. The
leak is reachable only from the states management exists to repair.

## Fix

- `SerialPortByteStream::write_all` no longer flushes. Handing the bytes to
  the driver is the contract; waiting for them to reach the wire is not, and
  that wait was the only unbounded call in the framing thread. `write` itself
  stays bounded — it polls for writability under the port's 100 ms timeout and
  surfaces `TimedOut`, which the thread already treats as a lost connection
  and exits on. `DeviceByteStream::write_all` now states the boundedness
  obligation for every implementor.
- The framing thread re-checks the shutdown signal **per queued message**, not
  per loop pass: a close means stop, not finish the queue. It also drops the
  stream explicitly at exit, where the release actually happens.
- `AsyncSerialClientTransport::close` documents that returning `Ok` *is* the
  promise that the resource is free, carries a backend label so its timeout
  error names the held port instead of "Backend thread", logs at `error!`, and
  waits 2 s (comfortably clear of the loop's own ~100 ms worst-case pass).
- The two callers that swallowed the failure now surface it: the host provider
  records a session log entry + `Error` diagnostic before propagating, and
  `DeviceSession::release_link` emits the failure onto the console feed the
  operator is already reading.

## Regression coverage

`lpa-client` `transport_serial::hardware::tests`:

- `close_drops_the_byte_stream` — pins the invariant itself: `close()`
  returning `Ok` means the stream (the port fd, on hardware) is gone.
- `close_abandons_a_write_backlog_instead_of_draining_it` — queues 60 writes
  at 50 ms each behind a silent stream, closes mid-drain, and asserts the
  close lands inside its join budget with the backlog abandoned. Verified to
  fail before the fix (`close: Other("serial backend thread for
  /dev/test-backlog did not stop within 2.0s; it still holds the
  connection")`).

The `tcdrain` half has **no automated coverage**: it needs a real OS serial
port whose peer has stopped draining, and on macOS a pty cannot stand in for
one (`serialport` sets baud via `IOSSIOSPEED`, which a pty driver refuses with
`ENOTTY`). Its evidence is the bench walk plus the structural argument that it
was the only unbounded call on the thread.

## Lesson

A `close()` that cannot enforce its own postcondition is a request, not a
close — and if the caller treats it as best-effort, the resource leak surfaces
a whole operation later wearing someone else's error message. Two rules fall
out. First: when one thread is the sole owner of an OS resource, every call it
makes must be bounded, or its shutdown signal is advisory; a library call that
looks like a formality (`flush`) can be the one unbounded call, so read the
implementation rather than the name. Second: the layers most likely to hide
this are the ones being polite — `let _ = close()`, "best-effort teardown" —
because a resource leak is exactly the failure whose symptom appears
somewhere else. Best-effort may describe the *outcome*; it must never
describe the *reporting*.
