# `lp-emu/esp/` — the Espressif SoC layer

This directory is the vendor namespace decided in vision D9: architecture
cores live at the root of `lp-emu/`, and anything that assumes a *chip* — a
memory map, MMIO decode, peripherals, a ROM image — lives under the vendor it
belongs to.

It holds **two machines**, and both boot the shipped firmware to its idle
loop:

```bash
just emu-c6 target/emu-ref/d6cfaa205-boot-idle-memfs/fw-esp32c6 --timeout 6s --strict-bus
just emu-esp32v3 target/emu-ref/<commit>-boot-idle/fw-esp32v3 --timeout 2s --strict-bus
```

## Two machines

They are the two sides of the engine extraction M2 did, and the interesting
part is the seam between what they share and what they cannot.

**Shared**, and shared *because* a second machine was built rather than in
anticipation of one: `lp-emu-esp-common`'s bus and MMIO decode, the
`Peripheral` trait and its `BusCx`, `RegFile`'s accept-and-remember with its
table of exceptions, the bus trace and its spin detector, host byte streams,
the interrupt-matrix seam, the **signal fabric** (pads, signals, edges,
outside drivers, pad-to-pad wires) and the WS281x decoder over it, the ELF
view, and the four peripheral **engines** — UART, TIMG, SPI-flash and SHA —
each of which holds the behaviour and none of which holds a register offset.
Under that, `lp-emu-core`'s guest memory, scheduler and cycle model.

**Not shared**, and each of these is a place the two chips genuinely differ:

| | `lp-emu-esp32c6` | `lp-emu-esp32v3` |
|---|---|---|
| hart | `lp-riscv-emu`, RV32IMAC | `lp-xt-emu`, Xtensa LX6 — register windows, a vector table, `PS` instead of `mstatus` |
| cores | one | **two slots**, core 1 held by a three-part stall key (M3 runs single-core) |
| register layouts | the `esp32c6` PAC | the `esp32` PAC. Almost nothing lines up: 40 pads in two banks against 31 in one, 256 input signals against 128, a `TEXT` window that is both message and digest, `LACT` as a clock |
| the boot path | ROM → app | ROM → **the real ESP-IDF second-stage bootloader** out of a merged image → app, and the log is compared line for line against silicon |
| the host link | a USB-Serial-JTAG peripheral *inside* the SoC — a client connecting **is** the port opening | a **CH340 bridge chip on the board**. Opening the port moves no chip state; what resets the chip is the auto-reset circuit driven by the modem lines, and the truth table is the board's |
| memory | one flat HP SRAM | SRAM0 with a **measured word-only rule**, SRAM1, SRAM2, two RTC blocks, two flash windows through a per-core cache MMU |
| the `rmt-chase` payload's chip half | `output::LedChannel` on GPIO18, `RMT_SIG_0` = **71**; the driver swaps RGB→GRB and `lp-ws281x` permutes again, so the wire carries the caller's RGB | the product's own `shared_driver` + `v3_rmt` on IO18 (there is no `LedChannel` here — M4 ruling R5), `RMT_SIG_0` = **87**, which is `RMT_SIG_4` on this chip's *input* table; **one** permutation, so the wire carries real GRB and the gate unpermutes before checksumming |

Both machines carry that payload's gate — `lp-emu-esp32c6/tests/rmt_chase_replay.rs`
and `lp-emu-esp32v3/tests/rmt_chase.rs` — and both say the same thing about
768 frames: the guest's own FNV-1a of the buffer it handed the driver equals
the decoder's FNV-1a of the bytes the pad carried. **On both chips, today,
both readings are ours**; the silicon twin of the classic's transcript is M5's.

A change to anything in the first list is a change to both machines, which is
why CI's `emu_c6` and `emu_esp32v3` path filters both fire on `lp-emu/**`. A
change that moves one and not the other is exactly what nobody would notice.

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
  seam, the machine-request slot, the **signal fabric** (pads, signals and
  edges — where an output actually goes, and since M2 which pad an input
  signal reads and what an outside driver or a pad-to-pad wire holds on a
  pad) with the WS281x decoder that reads it, and the PT_LOAD view of an
  ELF. It holds **no chip numbers** — see its
  README.

- **`lp-emu-esp32v3/`** — the classic ESP32 (v3, LX6) machine: the same shape
  on an Xtensa hart, with the mask ROM's own boot chain and the real IDF
  second-stage bootloader behind it, a modelled flash chip and cache MMU, the
  CH340 cable on its control channel, and the 40-pad fabric. It is the only
  place classic chip numbers live — see its README.

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
cargo run -p lp-cli -- validate run emu-m4 --config lp-emu:esp32c6:t1 --dry-run
```

Door 3 is the one that makes a claim. `lp-emu:esp32c6:t1`, `:t2` and `:t3` are
configurations in `lp-emu/lp-emu-validate/validate.toml`, the runner's driver
turns a payload into exactly the command line door 1 takes, and
`validate record` writes the capture under `lp-emu/transcripts/` with a
sidecar. Nothing about a run is decided anywhere else: the time grade, the
eFuse identity, the strict bus, the sentinel and the emulated timeout are all
on that command line, which is why `--dry-run` printing it is the whole
protocol.

## What it is trusted for

Every field class of `lp-emu:esp32c6:*` is graded **`modeled`**, each with its
reason in `validate.toml` — with one exception since M1 P4, `t3`'s `timing`
row, which is `documented` **within a stated band**. That is not modesty and
it is not a placeholder:

| class | why it is `modeled` |
|---|---|
| memory | byte-equal to silicon on the compile harness — 372/372 values — which is *evidence in the reason*, not a promotion. `measured` would want a transcript per class |
| timing | `t1` counts instructions, `t2` uses a per-class model, `t3` adds the flash cache's fills and the APB's wait states on top of class costs the `cycle-probe` kernels measured. `t1` and `t2` stay `modeled`. **`t3` is `documented` inside a band** — the first use of a trust entry's `band` field, which is what a class that is never exact is graded against instead of equality: 85 of 92 like-for-like silicon slices inside [0.80, 1.25] with an aggregate of 1.154, on `shader-compile-stress` and `cycle-probe` and nowhere else, replayed by `--strict-timing`. Not `measured`: the compute residual is one-signed (the model is systematically cheap, with three named unmodelled causes), and `cycle-probe` is the payload the model was calibrated on, so the band rests on one independent workload. A second payload is what promotes it. Derivation: `docs/reports/2026-09-08-esp32c6-t3-calibration.md` §4; the rule: the hardware-validation ADR's 2026-09-08 amendment |
| boot-log | `modeled` on a direct load, which prints no banner at all; **`measured`** on a `--merged` ROM-up boot, whose log is diffed line for line against the committed silicon capture |
| usb-serial-jtag | the host's three states and the transitions between them, with four committed transcripts behind them (M6) — `boot-idle` over the shipped link against silicon's capture of the *same image bytes*, the port held closed from boot, an unplug mid-session, and no cable at all. M6 P5 adds the working half: the shipped image from flash takes a real `lp-cli upload` over this link, and every filesystem write, heap gate and compiler output is identical to the same script run over UART0. The block's own data path is graded `measured` register by register in `periph/usb_sj.rs`; the **class** stays `modeled` by the rule below. The silicon transcript that would let anybody argue otherwise has landed (`usb-negative-control/silicon-…-b18360ea6.txt`); whether it promotes the class is the director's to rule |
| pin | the waveform is a modelled peripheral's, decoded by our own decoder (M5 P2), and two pin captures are committed beside their transcripts: on `rmt-chase` the guest's per-frame checksums agree with the pad on all 768 frames (M5 P3), and on `shader-oracle-walk` the **shipped** image's first lit frame off gpio18 is byte-equal to the host oracle's `[ORACLE] rgb=` / `[ORACLE-RV32] rgb=` line, with every later frame the same (M5 P4). The oracle never touched the machine, which makes it the nearest independent check there is; it is still not a measurement, because both readings of the *pad* are ours. A silicon pin transcript — a logic analyser, or M8's C6 `frame-dump` port read beside the decoder — is the `measured` step |
| wire | the bytes are the guest's; a live socket's arrival times are the host's |

The rule behind the table is the vision's: **never trust an emulated number
outside what a transcript proves.** Note the two levels it now runs at. A
*class* is graded here, for a whole configuration, and a *register* is graded
in its block's own table (`--strict-grade`); the second is finer and does not
promote the first. USB-Serial-JTAG is the worked example: six registers
measured, the class still `modeled`. The spike found esp-emu byte-equal on
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
Those are the deterministic path and the one gates use. And both USB sockets
are available over **WebSocket**, for N boards at once, through `lp-cli emu
serve` — see "The WebSocket door" below.

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
>  pin 20 1                 # a bench driver holds a level on a pad, from outside
>  pins                     # both sides of every pad, one line
<  ok pins cyc=160000 us=1000 pads=2 gpio18[route=sig71 ie=0 drv=- lvl=1 wire=gpio19] gpio19[route=- ie=1 drv=- lvl=1 wire=gpio18]
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
| `ok pins cyc=… us=… pads=<n> gpio<n>[route=… ie=… drv=… lvl=… wire=…] …` | every pad the machine has anything to say about — routed, input-enabled, driven from outside, or tied by a `--wire`. `route` is what the pad's level follows (`gpio-out`, `sig71`, `~sig71` inverted, `-` for none), `ie` its input enable, `drv` the level an outside driver is holding (`-` for none) and `lvl` the resolved level |
| `err <reason>` | nothing was applied, and the reason says why: an unknown command, a bad argument, `open` with no cable, `close` on a closed port, `reset` while `chip_rst` bit 2 (the guest's own "no chip reset from the serial channel") is set, a `pin` on a pad this run may not drive |

`pin` and `pins` need no USB block: the pads are the bus's, so they work on
a machine whose only console is UART0. `pin` refuses GPIO9, 12, 13, 16, 17
and 18 by name, exactly as `--pin-script` does.

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

### The WebSocket door: `lp-cli emu serve`

The two sockets above are TCP, one machine per process, bound before the run
and gone with it. A browser cannot open a TCP socket, and a browser is what
plan two's Studio walks need — so there is a third door, and it lives in
**`lp-cli`**, outside this fence:

```text
GET  /boards                 → the registry, as JSON (Access-Control-Allow-Origin: *, so a Studio page on another origin can fetch it)
WS   /board/<id>/bytes       → binary frames both ways; the payload IS the bytes
WS   /board/<id>/control     → the line protocol above, verbatim
```

```sh
lp-cli emu serve --board c6-a=target/emu-ref/…/fw-esp32c6 \
                 --board c6-b=target/emu-ref/…/fw-esp32c6 \
                 --listen 127.0.0.1:5599 --state-dir target/emu-serve
lp-cli upload projects/test/basic serial:ws://127.0.0.1:5599/board/c6-a/bytes
```

It is a **pump, not a translation**. Each board runs on its own thread with
`--usb-sj tcp:127.0.0.1:0` and `--control tcp:127.0.0.1:0` — ephemeral
loopback ports, read back through `Esp32C6Machine::usb_sj_tcp()` and
`control_tcp()` — and the WebSocket endpoints move bytes and lines between a
socket and those ports. Nothing under `lp-emu/` changed for it, which is the
point: the TCP client the pump opens **is** the byte client the coupling rule
watches, so connect ⇒ `open`, disconnect ⇒ `close`, `attach`/`detach` never
implied and one reply per command are all this machine's own behaviour rather
than a re-implementation of it. One byte client and one control client per
board at a time, as `TcpHost` has; a second is refused with `409`.

What `serve` adds beyond `run`: a registry of N named boards, one **eFuse
MAC** each (the desk board's with the last octet stepped, or `mac=` on the
`--board`), one persistent **flash file** each under `--state-dir` written
back on a cadence and on shutdown, a **console transcript** each under
`--console-dir`, `--reboot-on-reset` **on** (see above), and `--air <addr>` —
a one-way `LPA1` tap that is **auditable only** and never a transcript.

**Three board kinds**, `kind=` on a `--board`, differing in which entry the
hart takes and whether the chip keeps its writes:

| kind | entry | chip |
|---|---|---|
| `elf` (default) | the ELF's entry point, loaded straight into memory | a separate, initially empty flash part |
| `merged` | the reset vector, through the real mask ROM | the whole merged image, **read-only** — it is the image a gate named, so `--state-dir` is ignored |
| `rom-up` | the reset vector, through the real mask ROM | the board's OWN flash file, which keeps its writes |

`kind=rom-up` is the only shape that can be **flashed and then boot what was
written**, which is what plan two's Studio walks do through esptool-js. Its
image is a whole-chip image the flash file is *seeded* from the first time;
the word `blank` means a chip with nothing on it (a file actually named
`blank` is still reachable as `./blank`). A blank chip does not reach the
download console on its own — the mask ROM prints `invalid header:
0xffffffff` forever, as it does on the part — but the host's reset dance puts
it there, which is what every flasher does first anyway.

`GET /boards`'s `flash` word (`blank` / `loaded` / `merged`) is about the
**chip**, never about what the board is running: it asks the question the mask
ROM asks — is there an image magic at the reset vector — and it is recomputed
on every flush, so `blank → flash → loaded` is a sequence you can watch. A
`kind=elf` board runs an image that was never in its flash and truthfully
reports `blank` for its whole life.

`--usb-host` decides what a byte client finds. `attached` (the default, and
`emu run`'s) is the cable in with the port open from power-on, so the boot
console is on the wire and the first client is replayed it. `attached-idle` is
the cable in with the port **closed**, which is what makes the coupling rule
visible on `state` — at the cost of the boot log, because the firmware does
not write while nothing is draining, exactly as a board does not.

### The tab host: the `emu_*` slice ABI

The three doors above are all sockets. A Web Worker has none — and a Studio
tab is where the fourth door leads: the machine compiled to
**`wasm32-wasip1`**, instantiated by a page, and driven one guest slice at a
time from JavaScript.

**One artifact.** It is the same CLI binary the bench rig builds, and
`_start` is untouched — `lp-emu esp32c6 run …` still works under a WASI
runtime. Beside it, added to the export list **at link time**, are twenty-three
`emu_*` functions. The JS host never calls `_start`; it calls the exports.

```sh
just emu-c6-wasm          # scripts/emu/build-tab-wasm.sh: build, then verify
just studio-emu-sidecar   # lay it down where a served Studio can fetch it
```

The build script holds the export list, and after the build it **parses the
module's export section and fails if any name is missing**. That is the gate:
a rename in `lp-emu-esp32c6/src/tab_abi/` and a stale list in the script is a
drift no native test can see. `--undefined` is passed alongside `--export`
for each name because the exports live in the library crate, which reaches
the binary as an archive, and a linker pulls archive members in only when
something roots them.

#### The exports

Every one takes and returns `i32`/`i64` only; buffers are a pointer and a
length into the module's own memory, from `emu_alloc`. A non-negative return
is a count, a length, a flag or an outcome code; a negative one is an error
code, and `emu_last_error` carries the sentence.

| export | what it does |
|---|---|
| `emu_abi_version` → `i32` | `1` today. The JS side asserts it and refuses a mismatch out loud |
| `emu_reply_max` → `i32` | the largest control reply, so the host sizes one scratch buffer |
| `emu_last_error(out, cap)` | the sentence behind the last negative return |
| `emu_alloc(len)` / `emu_free(ptr, len)` | buffers JS writes into before a call |
| `emu_create(cfg, cfg_len, flash, flash_len)` | build the machine from the text config below and the chip's starting bytes (`FlashBacking::Bytes`; an empty slice is a blank chip). Refuses a second — one board per module |
| `emu_create_direct(…, app, app_len)` | the same with an application ELF for `boot=direct` |
| `emu_run(budget_cycles)` → outcome | run at most that much **guest** time |
| `emu_cycles` / `emu_micros` / `emu_reboots` | the counters the host paces and reports on |
| `emu_control(line, len, out, cap)` | one line of the control protocol above, verbatim; the reply's text into `out` |
| `emu_usb_write(ptr, len)` | host → guest bytes, delivered at the next slice boundary the USB block polls at |
| `emu_usb_read(out, cap)` / `emu_uart0_read(out, cap)` | guest → host bytes; what does not fit is kept for the next call |
| `emu_flash_len` / `emu_flash_read(off, out, len)` | copy the chip out, whole or in chunks |
| `emu_flash_write(off, ptr, len)` | a **flasher's** write: erase every sector the range touches, then place the bytes |
| `emu_flash_erase_chip()` | the `Erase` verb |
| `emu_flash_dirty()` / `emu_flash_mark_saved()` | the persistence cadence's question, and the host saying it has answered it |
| `emu_flash_has_image()` | the `flash` word's `blank` / `loaded`, asked of the chip |
| `emu_destroy()` | drop the machine |

**Outcome codes**, `emu_run`'s return: `0` deadline (the ordinary answer —
the slice ran out), `1` exit-matched, `2` fault, `3` strict-bus, `4` reset
that was not rebooted, `5` breakpoint, `6` wall timeout (unreachable here,
and numbered so a seventh outcome cannot take a taken number). A machine that
returned a stopping code stays stopped and answers the same code again.

**Error codes**: `-1` no machine, `-2` already created, `-3` bad config,
`-4` build failed, `-5` buffer too small, `-6` bad buffer, `-7` out of range,
`-8` unsupported.

**The config** is one `key=value` per line, `#` comments allowed. An unknown
key is refused by name rather than shrugged at, because a host that misspells
one would otherwise get a default board and no idea why.

| key | values | default |
|---|---|---|
| `mac` | `aa:bb:cc:dd:ee:ff` | the builder's own |
| `boot` | `rom-up`, `direct` | `rom-up` |
| `grade` | `t1`, `t2`, `t3` | `t1` |
| `flash_len` | bytes | 4 MiB |
| `strict` | `0`, `1` | `0` — a board is not a bring-up run |
| `reboot_on_reset` | `0`, `1` | `1` — a reset dance reboots the chip rather than ending the world |
| `usb_host` | `absent`, `attached`, `attached-open` | `absent` — the cable is the host's to plug in, with a control line |
| `strap` | `app`, `download` | `app` |
| `reset_cause` | `poweron`, `usb-uart-hpsys` | `poweron` |

#### Time, and the one thing that is not in it

**Wall clock never enters the machine here either.** `emu_run` takes a budget
in guest cycles and the slice carries no wall timeout: the host is the only
party that knows what wall time is, and it converts on its own side of the
wall. A tab that was hidden and throttled therefore falls *behind* — which is
honest, and reportable as a number — rather than resuming with a sprint.

The module declares eighteen `wasi_snapshot_preview1` imports and, on this
path, **calls exactly one**: `clock_time_get`, which is `run_until`'s
unconditional `Instant::now()`. It cannot influence a slice that carries no
wall timeout. The rest are there because the binary also has a `_start` that
reads files and arguments, and the host stubs them.

#### Why no `_initialize`

The exports are callable with `_start` never called, on the plain command
binary: no `-Z wasi-exec-model=reactor`, no reactor `_initialize`. That is
measured, not assumed — instantiate the module with a stub WASI shim, call
`emu_abi_version`, `emu_create` with a blank chip on the download strap, and
`emu_run`, and `emu_uart0_read` hands back the mask ROM's own
`ESP-ROM:esp32c6-20220919` banner and its `waiting for download` line. The
machine's static slot needs no runtime initialisation because there is none
to run: it is one `UnsafeCell` in a `static`, valid precisely because the
wasip1 build is single-threaded and every export runs to completion before
JavaScript regains control.

A blank chip on the **app** strap says something different and equally real:
the banner, then `invalid header: 0xffffffff` forever, exactly as the part
does (see `kind=rom-up` above). The download strap is how a smoke reaches the
console without a firmware image.

#### Where the module goes

`just studio-emu-sidecar` copies it to the Studio assets tree, and
`scripts/sync-engine-sidecar.sh` content-hashes it into a served `pkg/` as
`lp_emu_esp32c6-<hash>.wasm`, naming it in `pkg/engine-manifest.json` under
`emu_esp32c6_wasm`. Its **absence is not an error**: no key, and a Studio
build that never ran `just emu-c6-wasm` serves normally without an emulator.

The consumer is `lp-app/lpa-studio-web/public/lpa-link/emulator_worker.js`,
which is AGPL and on the other side of the MIT fence — it reaches this module
by URL, and nothing here knows it exists.

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

Two lines carry no absolute time at all, and they are what makes a **walk**
a walk (M6 P5):

```text
after "[RECOVERY] boot complete (first frame served)" "M!{\"id\":1,…}\n"
after "\"id\":1," +5ms "M!{\"id\":2,…}\n"
then  +2ms "…the next 64 bytes of the same request…"
```

`after` waits for something the device said, `then` paces the host after its
own last chunk. A client sends its next request when the answer to the last
one arrives, not at a wall-clock offset, and a script written as absolute
milliseconds is brittle one way and full of dead time the other. The needle
is matched against what a host **on this link** received, so one walk file
replays on either link — `lp-emu/esp/lp-emu-esp32c6/walks/examples-basic.script`
is run over UART0 by the `upload-walk` payload and over this socket by
`upload-walk-usb`, and the two transcripts' heap ledgers are equal to the
byte. The wait resolves in guest cycles, so the determinism below is
unaffected. A `wait` offset moves the absolute lines only: a wait-for step
has no absolute time to shift.

`--usb-script` is **repeatable** and the files concatenate in the order
given. A scenario's cable schedule is three lines the runner writes inline;
a walk's wire conversation is a 12 KB generated file; a link that carries
both should not force them into one.

**The emulator never builds a frame.** `lpc_wire::json::to_serial_line` is
the single framer for every `M!` line in the repository (PR #538) and
`lpc-wire` is a product crate the fence keeps out of `lp-emu/`. Scripts carry
bytes; the runner and `lp-cli/tests/emu_usb_hello.rs` build the frame and
write the script.

### Scripted input on the pads (`--pin-script`)

The same split, on the other kind of input. `--pin-script <file>` is this
grammar with `pin <n> <0|1>` where the bytes go — the same `after` / `then`
forms, the same `#` comments, the same file-order rule — plus two
**generators**, so a payload does not hand-write forty lines of contact
bounce:

```text
1500us  pin 20 0
after "[INIT] ready" +5ms pin 20 1
then +2ms pin 20 0
button 20 press at 1000 bounce 5 edges over 200 hold 10ms
encoder 21 22 8 cw from 4000 at 4000hz
```

**File order is not drive order here.** A byte script's file order is wire
order because a serial line has one; pads do not. Two pads move
independently, a `button` expands to a rest level at cycle 0 and a release
milliseconds later, and an `encoder` beside it covers the same stretch of
time — so the absolute steps between two waits are driven in **time** order,
and a file that reads naturally still writes a pin log whose timestamps go
forwards. An `after` or `then` is a fence: everything after it is still
after it.

**The leading number is microseconds here**, not the milliseconds a byte
script uses: a contact bounce is tens of microseconds and an integer grammar
should be able to say so. `after`/`then`'s `+<ms>` delays are unchanged, and
both columns accept an explicit `us` or `ms` suffix. An `after` needle is
matched against **either** console, because a pin script says nothing about
which link the payload prints on.

Both generators are deterministic functions of their parameters and the
exact edge list each produces is in
`lp-emu-esp32c6/src/pinscript.rs`'s module doc — a `button` rests high
(a pull-up with a normally-open button to ground: **pressed is low**), an
`encoder` walks the Gray sequence from `(0, 0)` one channel at a time.

### Determinism

A **script** is deterministic: its times are guest time, and two runs of one
script deliver byte-identical bytes at identical cycle counts (gate G3-2).
The same holds for `--pin-script`: two runs write byte-identical pin logs at
identical cycle counts (M2 P1's G1-3), and a scripted edge is stamped with
the *script's* cycle rather than the slice boundary the machine noticed it
at, so a pin log can be checked against the file that produced it.

A **socket** is not, and cannot be. A client's command is applied at
whichever slice boundary the poll landed on, which depends on the host's
clock; the reply says which cycle that was, so a session is at least
*auditable*. That is exactly the line between `--pin-script` and the `pin`
verb, and the two never blur. Wall clock still never enters the machine — it
changes when a run notices the outside, never how fast the run goes. Gates
use the scripted form; the socket form has one test
(`tests/usb_socket.rs`) whose job is to prove the plumbing, with wall
timeouts as its safety net.

### What plan two's shim maps onto this

Plan two shipped, and this is what ran: `lp-cli emu serve`'s WebSocket door in
front of these two sockets, a `navigator.serial` polyfill
(`lp-app/lpa-studio-web/public/lpa-link/virtual_serial.js` + `emulator_port.js`)
in the page, and esptool-js 0.6.0 flashing the packaged image ROM-up — all
under Studio's *unchanged* device stack. The design and its rules are
`docs/adr/2026-09-09-studio-device-stack-over-a-virtual-serial-port.md`. The
table below is that mapping.

<!-- TODO(E2, open with Yona): the walk runs against `lp-cli emu serve`, the
native path, where the wasm translator is off and no flag turns it on; whether
`emu serve` gets a translator passthrough (and whether a fifth oracle image
covers the ROM-download flasher-stub path) is unresolved — see the plan-two
director log's E2 and `NOTE-from-m7-translator.md`. Our own `emu_serve_flash`
and door tests remain the only gate on the stub path until then. -->


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

**One row that is deliberately absent: a reset does not re-enumerate the
port.** On this part the USB-Serial-JTAG controller shares silicon with the
CPU it resets, so the USB device survives every reset the serial channel can
ask for — which is why a real C6 can be flashed over Web Serial at all — and
the shim models that (plan two M5; the reasoning and the measurement are in
`public/lpa-link/virtual_serial.js`'s header). Only `detach` then `attach`
mints a new `SerialPort`. `Esp32C6Machine::reboot` puts the guest's clock back
to zero, which is how a host learns a reboot happened at all; what it must not
do is make the port vanish under the flasher that asked for it.

### `reset` and `download-mode`

Both end the run **unless `--reboot-on-reset`**. M7 shipped the boot chain and
the flag: with it the machine performs the reset — `Esp32C6Machine::reboot`
puts the chip back to its power-on state with the strap and the reset cause
re-seeded, keeps both consoles' bytes so a log with two boots in it is a
better record than one that lost everything before the reset, and the run
carries on with `reboots()` incremented. The flag is **off by default** on
purpose: three merged M6 scenarios read the exit code of a run that ended on
a reset as their evidence.

`lp-cli emu serve` is the one place it is on and not configurable off. A
server cannot lose a board to esptool-js's DTR/RTS dance, whose whole purpose
is to reset the chip — see "The WebSocket door" above.

Without the flag, the machine reports the request instead of performing it:

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
