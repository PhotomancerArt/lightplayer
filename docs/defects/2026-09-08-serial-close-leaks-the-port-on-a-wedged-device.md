---
status: fixed          # bench-confirmed through the erase step still PENDING
                       # (see "Verification status") — the board dropped off
                       # USB mid-walk and needs a physical replug
found: 2026-09-08      # how: hardware-walk (c6-bootloader-hang-walk.sh)
fixed: this change
area: lpa-client stream/serialport_stream + transport_serial (hardware framing thread, AsyncSerialClientTransport::close); lpa-link host_serial_esp32 provider + DeviceSession::release_link
class: lifecycle-ownership
related:
  - 2026-09-06-c6-analog-master-wedges-the-bootloader.md
  - 2026-09-04-pre-flash-hello-stamps-over-a-closed-port.md
  - ~/.photomancer/planning/lp2025/2026-09-06-1649-c6-first-flash-bootloader-hang/
---
# A serial close a wedged device could veto

**Symptom** — On the C6 first-flash bench walk
(`scripts/c6-bootloader-hang-walk.sh <mac>`), against a board deliberately
hung in its bootloader: `DeviceSession::connect` correctly reported
`Unresponsive { NoSerialOutput }`, and the very next step —
`manage(EraseDeviceFlash)`, whose first act is `release_link()` → provider
`close()` → `ClientTransport::close().await` → reopen the same port by name —
failed to open it:

```
failed to open /dev/cu.usbmodem13101 after 20 attempts: Device or resource
busy
```

Twenty attempts is the two-second retry `host_esp32_flash::open_port` already
carried for the macOS "just-closed `/dev/cu.*` is busy for a beat" window. The
port was not busy for a beat; it was held for good. Nothing in the walk log
said so: `release_link()` discarded the close error with `let _ =`, so the only
visible failure was the reopen, one step downstream of the layer that had
actually failed.

## Root cause

**A tty teardown WAITS for its output queue to drain, and a device that has
stopped reading never lets it.** That is one mechanism with three exits, and
the code walked into it at every one.

The framing thread spawned by `create_hardware_serial_transport_pair_with_options`
is the **sole** owner of the `DeviceByteStream`, so the OS port fd is released
at exactly one instant: when that thread returns. `AsyncSerialClientTransport::close`
knows this and joins the thread — but on timeout it gave up, returned `Err`,
and the thread kept running and kept the fd. "Closed" was a claim the caller
could not enforce, and the enforcement it needed was denied by the peer:

1. `SerialPortByteStream::write_all` called `port.flush()`, which in
   `serialport` 4.9.0 is `tcdrain(fd)` — *block until every queued byte has
   been transmitted*, with **no timeout** (the port timeout there only bounds
   `EINTR` retries).
2. Removing that flush did not fix it, it **relocated** it: `close(2)` on a
   tty drains too. A `sample` of the wedged process on the bench is the
   evidence, and it names both ends of the deadlock in one shot —

   ```
   Thread_26228831: lp-hardware-serial
     ... serial_thread_loop + 7716
       core::mem::drop<Box<dyn DeviceByteStream>>
         drop_in_place<SerialPortByteStream>
           drop_in_place<serialport::posix::tty::TTYPort>
             nix::unistd::close
               close  (in libsystem_kernel.dylib)      ← parked here

   Thread_26228825: com.apple.main-thread
     ... host_esp32_flash::prepare_lp_domain
       host_esp32_flash::open_port
         serialport::SerialPortBuilder::open_native
           nix::fcntl::open
             __open  (in libsystem_kernel.dylib)       ← parked here
   ```

   The framing thread is inside the close that would have released the port;
   the main thread is inside the open that is waiting for it.
3. The message drain (`while let Ok(msg) = client_rx.try_recv()`) ran to the
   end of the queue before the loop looked at the shutdown signal again.
   Readiness re-asks a hello every second for the whole ready budget (30 s on
   the walk), so a board that never answers leaves a deep backlog, and each
   of those frames costs the port's full write timeout against a peer that
   refuses them — minutes of writes nobody will read, past any close's join
   budget.

The `Unresponsive` path is what exposed it, and not by coincidence: a board
that reaches `Ready` drains its FIFO on every frame and queues nothing, so
neither the drain nor the backlog has anything to wait on. The leak is
reachable only from the states management exists to repair.

Two things make it nastier than an ordinary leak. The wait is
**uninterruptible** — `SIGKILL` did not reap the process, and when the kernel
eventually tore the fd down the C6 dropped off the USB bus entirely and needed
a physical replug. And the *first* attempt at a fix, dropping the flush on its
own, silently traded the hang for a **misclassification**: `write` then
surfaced its own `TimedOut`, the framing thread read that as a dead stream, and
`connect` reported the repairable board as `Gone` — a state no management
operation runs from — instead of `Unresponsive`.

## Fix

- `SerialPortByteStream` discards queued output at teardown —
  `tcflush(TCOFLUSH)` in `Drop`, and on the `reopen` path that closes the old
  fd by assignment. The close then has nothing to wait for. Throwing away
  unsent bytes is right *here* and only here: the stream is being torn down,
  a close abandons its write backlog by design, the peer is about to be reset
  or reflashed, and the alternative is not "the bytes arrive" (the peer is not
  reading them) but a thread that never comes back.
- `write_all` no longer flushes: `tcdrain` is the same unbounded wait reached
  from the write side, and handing bytes to the driver — not waiting for the
  wire — is the contract. `DeviceByteStream::write_all` now states the
  boundedness obligation for every implementor.
- A write that times out is `ByteStreamError::WriteStalled`, a **dropped
  frame, not a dead stream**: the framing thread logs it and keeps the link,
  so the readiness deadline gets to classify the board as `Unresponsive`.
- The framing thread stops draining its write queue at the **first stalled
  frame**: everything behind it is equally undeliverable, and the outer pass
  re-checks the shutdown signal immediately. Keyed on the stall and not on the
  shutdown signal on purpose — see "What the first attempt at this broke"
  below. It also drops the stream explicitly at exit, where the release
  happens.
- `AsyncSerialClientTransport::close` documents that returning `Ok` *is* the
  promise the resource is free, carries a backend label so its timeout error
  names the held port instead of "Backend thread", logs at `error!`, and waits
  2 s (clear of the loop's own ~100 ms worst-case pass).
- Both callers that swallowed the failure surface it: the host provider
  records a session log entry + `Error` diagnostic before propagating, and
  `DeviceSession::release_link` emits it onto the console feed. This is what
  turned the second bench run from a mystery into a diagnosis — the walk log
  read `device link release failed: host serial ESP32 port not released on
  close: serial backend thread for /dev/cu.usbmodem13101 did not stop within
  2.0s; it still holds the connection`, one line before the open that hung.

## Regression coverage

`lpa-client` `transport_serial::hardware::tests`:

- `close_drops_the_byte_stream` — pins the invariant itself: `close()`
  returning `Ok` means the stream (the port fd, on hardware) is gone.
- `a_stalled_peer_does_not_hold_the_close_hostage` — queues 60 frames behind
  a stream that costs a full write timeout per attempt and then reports
  `WriteStalled`, closes mid-drain, and asserts the close lands inside its
  join budget with the backlog abandoned. Verified to fail without the break
  (`close: Other("serial backend thread for /dev/test-backlog did not stop
  within 2.0s; it still holds the connection")`).

The drain-on-teardown half has **no automated coverage**: it needs a real OS
serial port whose peer has stopped draining, and on macOS a pty cannot stand
in for one (`serialport` sets baud via `IOSSIOSPEED`, which a pty driver
refuses with `ENOTTY`). Its evidence is the bench sample above.

## What the first attempt at this broke

The obvious version of the drain fix — *re-check the shutdown signal before
every queued message* — was wrong, and CI said so: it turned
`an_effect_that_outlives_its_activity_gives_the_wire_back_and_the_pump_resumes`
(`lpa-studio-core`) red, deterministically, and bisecting the change down to
individual hunks named that one hunk alone. The studio device bench runs the
fake device over this same framing thread, and its eviction-recovery leg
depends on a close landing mid-drain **finishing what it started**: cutting
the drain short let the rebuilt link's hello arrive inside the same window,
and a board the test expects to read `NotResponding` read `Ready`.

The correction is the distinction the first version missed. "Shutdown" is not
what makes a queued frame undeliverable; a peer that refuses it is. Keying the
break on the stall serves both: a peer still accepting gets the queue it was
already being handed, and a peer that is not gets abandoned at the first frame
it refused — which is the only case that could ever have blown the budget.

## Verification status

Bench-confirmed so far, on the induced fixture: the classification is right
(`Unresponsive { NoSerialOutput }`), and the leak is now *reported* at the
layer that causes it. The final leg — the walk getting past the erase — is
**not yet run against the `tcflush` fix**: the C6 dropped off the USB bus when
the wedged process was killed and needs a physical replug before
`scripts/c6-bootloader-hang-walk.sh A0:F2:62:85:A8:7C` can run again.

## Lesson

A `close()` that cannot enforce its own postcondition is a request, not a
close — and if the caller treats it as best-effort, the leak surfaces a whole
operation later wearing someone else's error message. Three rules fall out.
When one thread is the sole owner of an OS resource, every call it makes must
be bounded or its shutdown signal is advisory; the call that looks like a
formality (`flush`, and then `close` itself) is exactly the one that blocks,
so read the implementation rather than the name. When you remove one
unbounded wait, check whether you moved it rather than removed it — the tty
drains on teardown whichever syscall you reach it through, and the second
exit was invisible until a stack sample named it. And when you bound a loop,
bound it on the condition that actually makes the work pointless (the peer
refused) rather than on the event that prompted you to look (a close arrived):
the first version of this fix cut a drain short for a peer that was happily
reading, and a bench that depended on the difference caught it. And an unresponsive peer is
a *state*, not an error: mapping "it isn't reading" onto "the stream is dead"
turns a board management could have repaired into one it refuses to touch.
