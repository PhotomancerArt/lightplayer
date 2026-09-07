# `lp-emu/esp/` — the Espressif SoC layer

This directory is the vendor namespace decided in vision D9: architecture
cores live at the root of `lp-emu/`, and anything that assumes a *chip* — a
memory map, MMIO decode, peripherals, a ROM image — lives under the vendor it
belongs to.

Today it holds one machine, and that machine boots the shipped firmware to its
idle loop:

```bash
just emu-c6 target/emu-ref/d6cfaa205-boot-idle-memfs/fw-esp32c6 --timeout 6s --strict-bus
```

## The layering

The runner on top, and four layers under it, each knowing strictly less than
the one above. That is what lets a second chip reuse the bottom three.

```text
lp-cli validate            payloads, transcripts, replay, trust grading
  lp-emu-esp32c6           THE CHIP: memory map, ROM image, reset state,
                           peripherals with real behaviour, the run loop, a CLI
    lp-emu-esp-common      bus, MMIO decode, Peripheral + BusCx, RegFile,
                           trace + spin detector, host byte streams, ELF view
                             — no chip numbers, none
      lp-riscv-emu         the hart: RV32IMAC executors and, since M3,
                           machine-mode CSRs, traps, mret/wfi, triggers
        lp-emu-core        guest memory, the scheduler, CycleModel, StepResult
```

- **`lp-emu-esp-common/`** — the SoC substrate every ESP machine shares: the
  bus (`SocBus`), the MMIO decode table, the `Peripheral` trait and its
  `BusCx`, `RegFile` (accept-and-remember with a table of exceptions), the
  bus trace with its spin detector, host byte streams, the interrupt-matrix
  seam and the machine-request slot, and the PT_LOAD view of an ELF. It holds
  **no chip numbers** — see its README.

- **`lp-emu-esp32c6/`** — the C6 machine: the memory map (every base cited to
  esp-hal's linker script), the mask-ROM loader and its deliberately empty
  hook table, direct load with the state the bootloader leaves behind, the
  peripheral set the boot path touches, the discrete-event run loop, snapshot,
  and a CLI whose every timeout is emulated time. The generated register-name
  tables in `src/regs/` come from `scripts/emu/pac-regnames.py` and carry
  their provenance headers. This is where the chip numbers live — see its
  README.

- **`roms/`** — the vendored ROM ELFs (Apache-2.0, from `esp-rom-elfs`
  release `20260528`), with their LICENSE and checksums (vision D6). The ROM
  is loaded in **every** configuration, because the application calls into it
  at runtime whatever booted it (plan PD7): `rtc_get_reset_reason` before
  `.bss` is zeroed, `ets_delay_us` from every clock path, `memcpy` and the
  `str*` family because the linker resolves them there.

## How to run it

Three doors, in order of ceremony.

```bash
# 1. one image, whatever flags you are debugging with
just emu-c6 <elf> --strict-bus --trace UART0,TIMG0 --timeout 200ms

# 2. the machine's own gates (needs firmware; builds the reference images)
just test-emu-c6

# 3. a recorded, replayable run — the one that produces a transcript
cargo run -p lp-cli -- validate run emu-m3 --config lp-emu:esp32c6:t1 --dry-run
```

Door 3 is the one that makes a claim. `lp-emu:esp32c6:t1` and `:t2` are
configurations in `lp-emu/lp-emu-validate/validate.toml`, the runner's driver
turns a payload into exactly the command line door 1 takes, and
`validate record` writes the capture under `lp-emu/transcripts/` with a
sidecar. Nothing about a run is decided anywhere else: the time grade, the
eFuse identity, the strict bus, the sentinel and the emulated timeout are all
on that command line, which is why `--dry-run` printing it is the whole
protocol.

## What it is trusted for

Every field class of `lp-emu:esp32c6:*` is graded **`modeled`** in M3, each
with its reason in `validate.toml`. That is not modesty and it is not a
placeholder:

| class | why it is `modeled` |
|---|---|
| memory | byte-equal to silicon on the compile harness — 372/372 values — which is *evidence in the reason*, not a promotion. `measured` would want a transcript per class |
| timing | `t1` counts instructions, `t2` uses the per-class model, and no transcript grades either yet (the vision's graded ladder) |
| boot-log | a direct load prints no ROM banner and no bootloader lines at all; M7 boots from reset |
| usb-serial-jtag | the host's three states and the transitions between them (M6 P2/P3), every register in the block still graded `modeled` or `documented` in its own table; P4 promotes what the transcripts cover |
| pin | nothing is observed; M5 brings the RMT channels and the WS281x decoder |
| wire | the bytes are the guest's; a live socket's arrival times are the host's |

The rule behind the table is the vision's: **never trust an emulated number
outside what a transcript proves.** The spike found esp-emu byte-equal on
memory and 2.4x wrong on time in the same run, and the compile harness's own
5 ms slice budget passing there while failing on the board.

## The host side: byte sockets and the control channel (PD8)

A machine has two links, and each of them has an outside. A byte socket
carries bytes and nothing else — `lp-cli … serial:tcp://<addr>` is a plain
byte client, and in-band control would be a dialect every client would have
to speak. So the *host's* side of the USB link — the cable, the port, the
DTR/RTS lines, the reset dances — rides a **second** socket in a line
protocol of its own.

| link | bytes | control |
|---|---|---|
| UART0 | `--uart0 tcp:<host:port>` (listens, one client; the client's bytes are RX) | none. UART0's DTR/RTS belong to a USB-serial bridge chip, not to the C6 — plan three's classic board is where that lives |
| USB-Serial-JTAG | `--usb-sj tcp:<host:port>` (listens, one client; the client's bytes are the OUT endpoint's) | `--control tcp:<host:port>` |

Both are also available as **files**: `--uart0-script` and `--usb-script`.
Those are the deterministic path and the one gates use.

### The commands

One command per line, UTF-8, `\n`-terminated. One reply per command. A
command is applied at the next slice boundary — the same place a peripheral's
machine request is drained — and the reply names the guest cycle it took
effect at. **No `M!` line is ever sent or expected here**: this socket is the
cable, not the protocol on it.

```text
>  attach                   # cable in: bus reset, SOF starts, port closed
<  ok attach cyc=12345 us=77
>  detach                   # cable out: SOF stops
>  open                     # an application opened the port: the IN endpoint drains
>  close                    # it closed the port: committed packets sit where they are
>  dtr 1 | dtr 0 | rts 1 | rts 0      # one control line
>  signals dtr=0 rts=1      # both in one write, the way a UartBridge-style host does it
>  reset                    # what the hard-reset dance does: chip_rst + Reset { strap: App }
>  download-mode            # what the download dance does: Reset { strap: Download }
>  usb-write <hex bytes>    # host -> device bytes with no byte socket (tests)
>  state
<  ok state cyc=368480001 us=2303000 host=attached draining=true sof=on in_pending=0 out_queued=0
>  nonsense
<  err unknown command `nonsense` (expected one of: attach, detach, …)
```

Every reply is one line and begins with `ok` or `err`:

| reply | when |
|---|---|
| `ok <verb> cyc=<cycle> us=<micros>` | applied, at that guest cycle. `<verb>` is the word the client typed — `dtr`, `rts` or `signals` |
| `ok state cyc=… us=… host=absent\|attached draining=<bool> sof=on\|off in_pending=<n> out_queued=<n>` | the host's side. `in_pending` is bytes sitting in the IN endpoint that nobody has taken; `out_queued` is host bytes the guest has not read |
| `err <reason>` | nothing was applied, and the reason says why: an unknown command, a bad argument, `open` with no cable, `close` on a closed port, `reset` while `chip_rst` bit 2 (the guest's own "no chip reset from the serial channel") is set |

A precondition is checked before the model is touched, so a script that has
drifted out of step says so instead of quietly doing nothing.

### The coupling rule

**A client on the byte socket is an application opening the port**:
connecting implies `open`, disconnecting implies `close`. That is what
`lp-cli`'s readiness engine and a Web Serial `open()`/`close()` mean, and it
is why `lp-cli … serial:tcp://<usb socket>` works unchanged against
`--usb-host attached-idle` — the connect opens the port and the periodic
`Hello` establishes the session.

`--usb-sj-drain manual` decouples them: the socket then carries bytes only
and the control channel owns `open`/`close`.

**`attach` and `detach` are never implied by a socket.** A cable is not a
port open, and the whole point of modelling the host is that the two come
apart: a device can be plugged in with nobody reading it, which is the state
the firmware calls "not draining".

There is nothing to replay to a late client on the USB byte socket. With no
client the host is attached-idle or absent, so no packet is ever delivered
and no backlog accumulates; the backlog exists only for a client that
disconnects and reconnects while `draining` is held on by
`--usb-sj-drain manual`.

### The scripted form

`--usb-script <file>` is `--uart0-script`'s grammar with the control words
added. One entry per line, `<ms> <what>`, where `<what>` is bytes — a
double-quoted string (`\n \r \t \0 \\ \" \xNN`) or whitespace-separated hex —
or one of the commands above. `#` starts a comment. **File order is wire
order.**

```text
# the s7 shape (scripts/device-scenarios/s7-unplug-mid-op.json), device side
0      attach
0      open
1500   "M!{\"id\":1,\"msg\":\"hello\"}\n"   # built by to_serial_line, pasted
6000   detach
9000   attach
9500   open
```

The leading `<ms>` is absolute emulated time from cycle zero. A `wait <ms>`
line shifts every **later** line, so a relative script survives an edit.
Bytes and commands are told apart by the first token: a quoted string is
bytes, a known verb is a command, anything else is hex — and no verb is a
pair of hex digits, so the rule never guesses.

**The emulator never builds a frame.** `lpc_wire::json::to_serial_line` is
the single framer for every `M!` line in the repository (PR #538) and
`lpc-wire` is a product crate the fence keeps out of `lp-emu/`. Scripts carry
bytes; the runner and `lp-cli/tests/emu_usb_hello.rs` build the frame and
write the script.

### Determinism

A **script** is deterministic: its times are guest time, and two runs of one
script deliver byte-identical bytes at identical cycle counts (gate G3-2).

A **socket** is not, and cannot be. A client's command is applied at
whichever slice boundary the poll landed on, which depends on the host's
clock; the reply says which cycle that was, so a session is at least
*auditable*. Wall clock still never enters the machine — it changes when a
run notices the outside, never how fast the run goes. Gates use the scripted
form; the socket form has one test (`tests/usb_socket.rs`) whose job is to
prove the plumbing, with wall timeouts as its safety net.

### What plan two's shim maps onto this

The vocabulary is the scripted fake device's
(`lp-app/lpa-link/src/providers/fake_device/fake_device_core.rs`) so that the
browser shim is glue rather than a translation:

| Web Serial / esptool-js | control channel | the fake device |
|---|---|---|
| `navigator.serial.requestPort()` + `port.open()` | `attach`, then connect the byte socket (or `open`) | a device appears, then `reopen` |
| `port.close()` | disconnect (or `close`) | the stream is dropped |
| the user unplugs the board | `detach` | the device goes away — scenario s7 |
| `port.setSignals({ dataTerminalReady, requestToSend })` | `dtr 0\|1`, `rts 0\|1`, `signals dtr=… rts=…` | `set_signals(dtr, rts)` |
| esptool-js's hard reset (`D0; R1; D0; R1; R0`) | the same lines | an RTS falling edge with DTR never high = reset |
| esptool-js's download reset (`… D1 … R1 … R0`) | the same lines | an RTS falling edge with DTR seen high = download mode |
| `port.readable` / `port.writable` | the byte socket, both ways | the fake's byte stream |

The dances are **decoded**, not pattern-matched: the model watches the RTS
falling edge and whether DTR was ever high, exactly as `set_signals` does, so
any host tool whose sequence has that shape works without being listed here.

### `reset` and `download-mode` until M7

Both end the run. The chip would reboot, and the emulator does not yet have a
boot chain to reboot into (M7 owns it), so the machine reports the request
instead of performing it:

```text
RESET requested by USB_DEVICE chip_rst (serial) at cycle 96000000 (600000 us),
strap = download — the chip would reboot; the emulator reports it (M7 owns the boot chain)
```

Exit code **2**, with the strap named. `chip_rst` bit 0 is set (write-one-to-
clear, as the PAC says); if the guest has set bit 2 —
`disable_usb_serial_chip_reset` — the request is recorded and **not**
performed, and the control channel answers `err` naming the bit rather than
pretending.

## Everything here is MIT

Inside the fence — see `../README.md` and `just lint-emu-fence`. An `esp/`
crate may not reach a product crate, `fw-checks` included: the validation
system's payload registry deliberately **mirrors** `fw-checks` rather than
importing it, and `lp-cli` owns the parity test that keeps the two equal.
