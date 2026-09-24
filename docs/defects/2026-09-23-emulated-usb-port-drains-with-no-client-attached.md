---
status: fixed
found: 2026-09-23      # how: live-debugging, measuring wire bytes for the lean-wire plan (Run D)
fixed: this change
area: lp-cli emu serve (args.rs `ServeHost` default, serve/board.rs) × lp-emu-esp32c6 machine.rs (the byte-socket coupling) × lp-emu-esp-common host.rs (`TcpHost` backlog)
class: fidelity
related:
  - lp2025/2026-09-23-1501-lean-wire/ (notes.md § "Heartbeat backlog", p2-emulator-backlog-fix.md)
  - docs/adr/2026-09-09-studio-device-stack-over-a-virtual-serial-port.md
  - docs/adr/2026-09-10-the-emulator-first-device-walk.md
---
# The emulated USB port read from power-on with no client attached, so the first client was replayed everything the board said to nobody

**Symptom** — Run D of the lean-wire measurements: `just studio-dev-emu`
sat for about 25 minutes before Studio first connected to `c6-a`, and at
connect the byte socket delivered **299 heartbeats (~160 KB)** in one burst,
ahead of anything the board said in reply to Studio. The serve log's
`TcpHost: client … attached (replaying N B)` line said the same thing. A
board on a desk, plugged in for 25 minutes with no application holding the
port, delivers none of that.

**Root cause** — three things, each reasonable alone:

1. `lp-cli emu serve` defaulted to `--usb-host attached`, which is
   `UsbHost::Attached { draining: true }`: the cable in *and* an
   application draining the IN endpoint, from power-on.
2. The socket↔port coupling (`Esp32C6Machine::service_host`) only issues
   `open`/`close` on an *edge* of the byte client's connectedness. It starts
   at `false`, and "no client yet" is also `false`, so nothing closed the
   port that power-on had opened. Nobody was connected and the emulated host
   kept reading.
3. Every packet a draining host takes is written to the `TcpHost` sink, and
   with no client `TcpInner::write` appends it to a backlog of up to
   `TCP_BACKLOG_CAP` (4 MiB) that the first client is replayed.

So the firmware's writes all succeeded, its not-draining latch never fired,
and 25 minutes of heartbeats, plus the boot console and the boot hello, sat
in host memory for Studio. The doc comment on `UsbSjSink::Tcp` claimed
"there is nothing to replay to a late client … no backlog accumulates". That
was true for `attached-idle` and `absent` only, and it was not true for the
default.

**Why silicon differs** — on a board, "the cable is in" and "an application
is reading" are separate facts. With no application holding the port, the
host does not poll the IN endpoint. The 64-byte IN FIFO fills, two
consecutive 250 ms write timeouts latch "host not draining"
(`lp-fw/fw-esp32-common/src/serial/usb_connection.rs`), and from then on the
firmware drops protocol frames rather than queue them. The firmware allows
only one frame in flight (`transport.rs`). An application that opens the port
later gets at most the FIFO's stale bytes, and then fresh frames once the
firmware's probe write succeeds and the latch lifts. The emulator model had
all of this right: `Attached { draining: false }` behaves exactly like that.
The mistake was the door's default, not the USB model.

**Fix** — no client now means nobody is reading.

- `lp-cli emu serve --usb-host` defaults to **`attached-idle`**: the cable
  is in and the port is closed. A byte client connecting is the `open` and
  disconnecting is the `close`, and the existing edge semantics apply from
  there. `reboot()` already re-derives the coupling's memory from both the
  port and the socket, so a flasher's reset dance with a client connected
  re-opens the port on the next poll.
- **Boot-console decision (written at `serve/board.rs::build` and on
  `ServeArgs::usb_host`)**: the boot console is **not** replayed by
  default. Nothing silicon would have discarded is delivered. The bound is
  the model's own: whatever the IN FIFO still holds (≤ 64 B) when the port
  opens. The console is not lost to a reader, though. `--console-dir` still
  writes what the guest tried to say to `<id>.console-untaken.log`, which is
  the emulator's observation and never goes on the wire.
- `--usb-host attached` stays as the **explicit opt-in** to the old
  behaviour, documented as "an application had the port open since
  power-on" (up to 4 MiB replayed). That is a real state a board can be in,
  such as a monitor started before the reset. It is not what an unopened port
  does.
- Comments corrected: `UsbSjSink::Tcp` in `machine.rs` (when a backlog builds
  and when it cannot), `TCP_BACKLOG_CAP` in `host.rs`, and the reboot
  comments that called `attached` serve's default.

Consumers that had been relying on the replay were changed to behave like a
client of a real board:

- `lp-cli/tests/support/mod.rs` `Serve::hello` now **asks** for the hello
  (`ClientRequest::Hello`, re-sent on a short wall cadence until answered)
  instead of waiting for the replayed boot hello. That is what Studio and
  `lp-cli` do.
- `lp-cli/tests/emu_serve_walk.rs`'s second server reads the second boot's
  console, so it passes `--usb-host attached` by name: to see a boot log on a
  desk you must be listening before the reset.
- `lp-cli/tests/emu_serve_flash.rs`'s ROM-up test connects late, so it waits
  for the erased chip's *next* boot pass to arrive whole instead of reading
  the first `invalid header`, which can follow a torn FIFO fragment.

**Not changed, deliberately:**

- `scripts/emu/uart-tcp-proxy.py` has the same 4 MiB buffer-and-replay. It
  is left alone. It fronts UART0 (esp-emu's `--uart-tcp`, the classic's
  `--uart0 tcp:`) and `scripts/emu/upload-walk.sh`'s socket, and it is not
  on the Studio path. UART0 has no host-open concept: a UART transmits
  whether or not anything is listening, so buffering a boot banner for a
  transcribing proxy invents no dropped-frame semantics.
  `upload-walk.sh` runs the binary with `--usb-host attached`, the explicit
  opt-in above.
- The `lp-emu-esp32c6` binary's default is `absent`, and it is unchanged.
  `lp-cli emu run` had `attached` too; it gets the same fix, below.
- The Studio tab backing (`emulator_tab.js`, `usb_host=attached`) has no
  byte socket and no `TcpHost` backlog. It sends `open` itself.
- The committed `lp-app/lpa-link/testdata/device-traces/*.emu.failed.jsonl`
  captures predate this change and carry the replayed backlog in their `rx`
  records. They are failed traces, never golden fixtures (the replay test
  skips them), so they are left as captured. `trace-diff.mjs` never compares
  rx counts.
- No `lp-emu/transcripts/` capture moved. None is recorded through
  `emu serve`.

**Regression coverage** —
`lp-cli/tests/emu_serve_door.rs::with_no_client_the_default_port_is_closed_and_nothing_is_replayed`
(run by `just test-emu-serve`). It starts the **default** serve, waits three
heartbeat intervals of the board's own clock (`state`'s `us=` ≥ 15 000 000,
which is emulated time, not wall-clock) with no client, and asserts:

- the port is still `draining=false`;
- `in_pending` ≤ 64 (the IN FIFO);
- the first whole heartbeat the client reads has `uptime_ms` ≥ 10 000, so it
  was written after the connect, not replayed from the boot;
- no hello precedes it;
- ≤ 1 024 B precede it.

Measured on the reference image (`d6cfaa205`, `lp-emu:esp32c6:t1`):

- **fixed:** 28 B pending in the FIFO at attach, **139 B** before the first
  whole heartbeat, whose `uptime_ms` was 20 001;
- **old default:** 0 B pending (it had all been taken), **2 212 B** before
  the first heartbeat (boot console and boot hello), whose `uptime_ms` was
  5 000, so the test fails on the replayed heartbeat.

**The same fix for `lp-cli emu run --link`.** `emu run` defaulted to `attached` from
power-on however it was run, so a client that connected to `--link` late was
replayed the boot console and every heartbeat, exactly as `emu serve` had
been. The same rule now applies to it. When the USB-Serial-JTAG port *is*
the `--link` socket, the default is `attached-idle` and the client's connect
is the `open`. `emu run` now takes `--usb-host attached|attached-idle|absent`,
spelled as `serve` spells it, in place of `--host-absent`, so `attached` stays
an explicit opt-in. The default only changes where a socket client stands for
the application. Everywhere else a reader present from power-on is what the
run means, so it stays `attached`:

- `--monitor`, which declares a reader holding the port for the whole run
  and so refuses `--usb-host`. `m4-walk.sh` (`just walk-esp32c6-emu`) uses
  it, so the walk's console is unchanged.
- no `--link` at all. The emulator is the reader and `--console` writes what
  it read. This is `heap-budget-check.sh`'s run.
- `--link-kind uart0`, where nothing couples a client to the USB port
  (`two-binary-probe.sh`).

No test or script ran `emu run --link` over USB without `--monitor`, so
nothing that read the replayed console had to change. The resolution is
`handler.rs::usb_host_at_power_on`, unit-tested there. The machine-side
behaviour is the same configuration as `serve`'s default (`Attached {
draining: false }`, a `Tcp` sink, `UsbSjDrain::Auto`), which the regression
test above already covers end to end.

Measured by hand on the reference image (`d6cfaa205`, `lp-emu:esp32c6:t1`),
connecting to `emu run --elf … --link` 8 s of wall clock after start (the
wait is only a way to arrive late; every figure below is the guest's):

- **new default:** `replaying 0 B`, **139 B** before the first whole
  heartbeat, whose `uptime_ms` was 75 001, and no hello ahead of it. These are
  the same 139 B `serve`'s fixed default measured.
- **`--usb-host attached`:** `replaying 11391 B`, **2 212 B** (boot console
  and boot hello) before the first heartbeat, whose `uptime_ms` was 5 000.

`lp-cli upload projects/test/basic serial:tcp://…` against the new default
connects, compiles the shader on the guest and reports "Project uploaded and
running". The client asks for its hello and needs no replayed one.

**Lesson** — "cable in" and "port open" are two facts, and a door that
couples a socket to one of them must start both from the same place. An edge-
triggered coupling is only correct if its initial state *agrees with* the
power-on state it couples to. Here the coupling started at "no client" and
the port started at "open", and no edge ever reconciled them. The same
mismatch was already caught once at reboot (`reboot()`'s CLIENT term). The
check to make whenever a coupling is edge-triggered: at power-on, is the
remembered side the same as the actual side? If not, either derive one from
the other or make the default agree. Also: a doc comment that states an
invariant ("no backlog accumulates") needs to name the configurations it
holds for.
