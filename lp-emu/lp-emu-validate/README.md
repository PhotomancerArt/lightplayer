# `lp-emu-validate` — one hardware-validation system

Five mechanisms used to answer "does this work on the board", and each got one
thing right. This crate is the sixth only in the sense that it composes them:

| it existed | what it got right | where it lives |
|---|---|---|
| FP silicon replay | verbatim silicon captures as committed fixtures, replayed every test run, **never edit a capture** | `lp-emu/lp-xt-emu/tests/fp_silicon_replay.rs` |
| `fw-checks` + `lp-cli fwcheck` | named checks as a `no_std` crate, a config table, structured JSON records | `lp-fw/fw-checks/`, `lp-cli/src/commands/fwcheck/` |
| the FP harness rig | the same generator on host and device; drift is a hard stop | `lp-xt/lp-xt-fp-harness/` |
| device-scenarios golden traces | captures with an `expect` list, and a **port-holding runner** | `scripts/device-scenarios/` |
| `test_*` cargo features | coverage — at one full image rebuild and reflash per payload | `lp-fw/fw-esp32*/Cargo.toml` |

The shape is four nouns and one verb.

## Payload

A module in `fw-checks` behind a cargo feature, runnable many per image
(vision D14). It prints a canonical transcript: an in-band header line, then
records behind `[fw-check-json] `, then human log lines the host parses as
*series*.

**One payload is not a module**, and it is the interesting one. `boot-idle` is
the shipped image itself, run to its first idle heartbeat (vision Q1:
shipped-image walks are a distinct scenario kind). It has no `fw-checks`
feature, prints no in-band header — its provenance is the sidecar alone — and
its firmware features are the image's own. That is why a payload's features are
a *list* and why `emits_header` is a field rather than an assumption. Its three
series are the hello frame, the idle heartbeat and the stack probe's line, and
the numbers it exists for are `freeBytes`/`totalBytes` and the stack
high-water mark.

**One payload is a conversation.** `upload-walk` (M4) is the same shipped
image with a host on the other end: the thirteen wire frames `lp-cli upload
examples/basic` sends. Its host half is a field —
`host_script: Some("lp-emu/esp/lp-emu-esp32c6/walks/examples-basic.script")` —
which an emulated configuration's driver passes as `--uart0-script`, and
which on silicon is the client itself over a port. Recording it on the
*payload* is what makes a walk reproducible at all: without it the only
transcript a runner can produce is a boot. The script is generated from a
real client capture, never hand-written (`walks/README.md`), and each of its
requests waits for the answer to the one before it, so the run is a function
of guest time and two recordings are byte-identical.

That is also the payload whose series are the most interesting: `fs-write`
(every file the upload wrote and the device's answer to each), `load-gate`
(the server's four heap gates, all `Memory` — and all byte-equal to the spike
report §5.3's) and `shader-compile` (the compiler's outputs `Structural`, its
`elapsed` `Timing`).

### What a payload says about the host (M6)

Four more fields on `Payload` are host-side rather than firmware-side, and
each exists because a scenario could not be expressed without it. (They join
`host_script`, above: that one is the UART0 walk's *wire conversation*, these
are the USB link's *cable*. M6 P5 is where the two links' scripts meet, and
where it will be worth asking whether they should be one field.)

- **`link`** — `UsbSerialJtag` or `Uart0Spike`. Until M6 every emulated run
  got the `spike_uart0_link` feature, which moves the host link onto UART0,
  because "neither emulator has a USB host". That stopped being true, and it
  mattered: comparing a UART0-link image on our machine with a USB-link image
  on the board compares two link drivers' allocations, not two machines
  (DD30). `esp-emu:*` still gets the workaround unconditionally — its USB
  model asserts SOF for ever (spike report §4) — and so do the two payloads
  whose committed transcripts are of that image. A transcript is never
  re-baselined to suit a later idea.
- **`host_plan`** — the `--usb-host` state a run starts in and the
  `--usb-script` that follows, in absolute emulated milliseconds. The script
  is written by a plan *step*, so its whole text lands in the sidecar's
  `source`: a scenario referenced only by a path is a scenario nobody can
  check. Sockets are host time and have no place in a transcript.
- **`run_secs`** — a scenario is a schedule ("the port opens at eight
  seconds"), and one `--timeout-secs` across a whole set cannot say so.
- **`emulator_only`** — the reason silicon cannot record it, in a sentence.
  `usb-host-absent` is the one: an absent host records nothing, because
  recording is what a host does. The silicon and esp-emu drivers refuse it
  with that sentence rather than writing an empty file and calling it
  evidence.

`Sentinel` grew a third shape for the same payload. `Done` is a line the
device prints and `Ready` is a line it prints before serving for ever;
`State` is for a payload whose subject is machine **state** — with no cable
the device says nothing at all, which is the finding, so the transcript is
the machine's own `--probe` report and no `--exit-on` is passed.

The M6 set is `emu-m6`: `boot-idle` (the shipped image with a host attached
and reading), `usb-negative-control` (the port held closed from boot),
`usb-detach-reattach` (the cable out mid-session and back in) and
`usb-host-absent`.

The registry is `src/payload.rs`. It **mirrors** `fw-checks` rather than
importing it: `fw-checks` is AGPL and outside the `lp-emu/` MIT fence, so an
import would fail `just lint-emu-fence`. `lp-cli` depends on both and owns the
parity test (`lp-cli/tests/validate_registry_parity.rs`), so the duplication
cannot drift silently.

## Configuration

The named reference implementation a payload ran on (plan PD4):

```text
silicon:esp32c6                 real silicon, that chip
esp-emu:0.42.0                  Espressif's binary emulator, that version
lp-emu:esp32c6:t1               our machine, time grade 1 (instruction count)
lp-emu:esp32c6:t2               our machine, time grade 2 (per-class model)
```

**Identity is the chip, not the board** (Yona, G2 2026-09-06). This is chip
simulation, not board simulation: what an emulator has to get right is the SoC.
The board is also not something the runner can determine programmatically — a
XIAO C6 and any other C6 enumerate identically — so putting it in the key would
have meant a human typing it correctly every time for no gain. Which board a
capture came from lives in that transcript's sidecar as `board`, beside `mac`
and `silicon_rev`, where it is a fact about one capture rather than part of a
name. `Configuration::parse` refuses a `/` in a silicon or `lp-emu` detail for
exactly this reason.

`validate.toml` says what each is trusted for, per field class, **with a
reason**. A class with no entry is `modeled`: silence is not trust. That table
is where the spike's findings live — esp-emu is `measured` for memory (byte-
equal on 184 per-tick values) and `modeled` for time (2.37x fast) and for
USB-Serial-JTAG (asserts SOF forever).

Our own machine is `modeled` in **every** class, and will stay that way until
each is earned separately. It is byte-equal to silicon on the compile
harness's 372 memory values, and that sentence is in the `because` where a
reader can weigh it — evidence, not a promotion. `measured` means the class was
measured on silicon, or on a configuration whose agreement with silicon *for
that class* is itself in a committed transcript; one payload's heap ledger is
not a licence for the pin class.

An emulated configuration also carries the identity it has no eFuse to read
(`mac`, `silicon_rev`, `board`), which the runner passes to the machine. The
configuration is still the chip: those are facts about the board being
imitated, not part of the name.

## Transcript

Committed, verbatim, under:

```text
lp-emu/transcripts/<chip>/<payload>/<configuration>-<date>-<short-commit>.txt
lp-emu/transcripts/<chip>/<payload>/<configuration>-<date>-<short-commit>.txt.meta.json
```

The `.txt` is the bytes as captured — ANSI escapes, espflash progress bars, ROM
banners and all. The `.meta.json` sidecar is the header, and it is required: a
transcript with no provenance is not a transcript. It carries chip, silicon
revision, board id, MAC, firmware commit and feature set, date, tool versions,
the capture method, and the trust table.

`firmware_sha256` (added by L4, **additive**: optional, no schema bump, and
every committed sidecar without it still loads) is the sha256 of the image the
run loaded. The commit says which source ran; this says which bytes did, and
those were not the same question until the reference recipe became
reproducible — M5 P1's digest caught three CI runs of one pinned firmware
commit producing three different ELFs (`scripts/emu/build-reference-image.sh`
names all three causes, and its `--verify` proves they are gone). A sha here is
now something another host can reproduce.

When the payload also printed an in-band header (`[fw-checks-header] {…}`), the
two must agree; `Transcript::load` refuses them if they do not.

**Never edit a transcript.** A mismatch is a regression or a re-capture, never a
fixture to refresh. That is the FP replay rule, and it is the only reason a
committed capture means anything.

### The pin capture (M5 P3)

A third file, for the payloads whose claim is about a **wire** rather than
about what the device said:

```text
lp-emu/transcripts/<chip>/<payload>/<configuration>-<date>-<short-commit>.txt.pins.jsonl
```

One decoded frame per line, as `lp-emu-esp32c6 --dump-frames` writes them, and
the sidecar's optional `pins` field is its **file name** — resolved against the
transcript's own directory, so a tree can be moved wholesale and a companion
can never point outside it.

It exists because a console capture cannot hold it. Everything else in a
transcript is something the firmware chose to say; this is what a pad carried,
decoded from the waveform by something that never spoke to the firmware. So
`rmt-chase`'s `[fw-check-json] {"kind":"rmt-frame","crc":…}` — the driver's
claim about the frame it handed the hardware — can be checked against
`{"kind":"ws281x-frame","wire":…}`, the bytes that actually went down the wire.
`Transcript::pin_records()` reads it and `replay` compares the frames as
**Pin**-class claims, which fail a replay; a transcript whose own guest and own
pad disagree is a structural problem naming the frame.

**Additive, by construction** (E3, approved 2026-09-07). Every sidecar written
before it loads unchanged, and only a payload whose registry entry sets
`pin_capture` is recorded with a companion at all — a configuration that cannot
observe a pad records the console half and says nothing about the wire, which
is what its trust grade already said.

## Replay

`replay(left, right, options)` compares the two field by field, each comparison
carrying its class:

* a difference in **memory**, **pin**, **wire**, **usb-serial-jtag** or
  **structural** fails the replay;
* a difference in **timing** or **boot-log** is reported *with its ratio*, not
  failed — time is where a configuration is allowed to be wrong, and hiding it
  would be the mistake;
* `--strict` refuses any class either side grades below `measured`.

Masking (`src/mask.rs`, the `scripts/spike/esp-emu/mask-transcript.sh` rules as
code) names the differences that mean nothing — heap digits in prose, uptime,
fps, tick counters, bootloader timestamps — each with the reason it is ignored.
Masking here means *reported but not compared*, never *deleted*.

## Runner

```bash
cargo run -p lp-cli -- validate list
cargo run -p lp-cli -- validate replay <transcript> --against <transcript|configuration>
cargo run -p lp-cli -- validate run <set> --config <name> [--port …] [--image …] [--dry-run]
cargo run -p lp-cli -- validate record <set> --config <name> --date … --commit … [--dry-run]
```

`--dry-run` prints the exact commands and stops, which is what makes a desk
protocol reviewable before a board is plugged in.

For silicon the runner does not reinvent the port discipline; it shells out to
`scripts/spike/esp-emu/desk-espflash-step.sh`, which runs espflash in the
**foreground** under `script(1)` with a `SIG_DFL` exec shim, polls for the
payload's sentinel, SIGINTs **that pid only**, and post-checks `lsof`/`pgrep`.
Every clause there is a sitting that broke.

### One payload is watched differently, and that is the payload

`usb-negative-control` (M6 P1b) asks what the device did while **nobody** was
reading it, so a monitor at the flash would destroy the thing it measures.
`Payload::capture` says so — `Capture::FlashThenOpenAfter(8)` against every
other payload's `Capture::Monitor` — and the silicon plan becomes three steps
instead of one:

1. `scripts/emu/desk-flash-no-monitor.sh` — the same pre-check, foreground
   `script(1)` and post-check, with no `--monitor`: espflash exits and the port
   goes back to closed.
2. `sleep 8` — the measurement. The board is enumerated and undrained: its
   writes time out, the connection monitor latches, and both log lines about it
   are dropped by that latch.
3. `scripts/emu/tty-capture.py --dev … --until <sentinel>` — a non-resetting
   reader (`os.open` + raw termios, `HUPCL` cleared, DTR/RTS untouched), the
   same open Studio and lp-cli make. `--until` stops at the **end** of the
   first line containing the sentinel, because the figures worth capturing come
   after a sentinel that is a line prefix.

They are three plan steps rather than one wrapper script on purpose: a desk
protocol is only reviewable if `--dry-run` prints the whole of it.

For `esp-emu:*` it builds a merged image and runs the binary named by
`$LP_ESP_EMU` with `--exit-on` the payload's done marker (install per spike
report §10 — a checksum-verified release asset, outside the repo).

For `lp-emu:*` there is no port, no reset dance and no `lsof` pre-check,
because there is no board. Its plan is two commands — build the payload
image, then run the machine with a console pointed at a file — plus a third
when the payload carries a host script, and every decision that shapes the
run is a flag on the machine's command line, so the printed plan is the whole
protocol. Which console depends on the payload's `link`: a USB-link payload
gets `--usb-sj file:<capture>` (the bytes a reader on the silicon port would
see) with `--usb-sj-tried` beside it for what the guest handed over that
nobody took, and a spike-link payload gets `--uart0 file:`. That was the seam
M2 left as `driver::ConfigurationDriver`, and neither M3 nor M6 changed
anything else here.

`--image [<payload>=]<path>` runs an already-built image instead of building
one. It exists for provenance: the committed C6 transcripts are at firmware
commits a checkout does not build — `d6cfaa205` with `spike_uart0_link`
applied as a dirty tree for the M2/M3 ones, and for M6 two clean trees with
no cherry-pick at all — and `scripts/emu/build-reference-image.sh <features>
[<commit>] [<spike>|none]` is what reproduces them. It takes a payload name because a set runs several payloads and
a reference image is built per feature set. Silicon refuses it — a run that
flashes somebody else's ELF cannot honestly report `firmware_features`.

Every path a plan prints is relative to the repository root and the steps run
there, so the `source` line in a committed sidecar reads the same in anyone's
checkout.

## Rules of the desk

- **Never open the port while Studio holds it.** `just hardware-list` is
  passive; `--probe` resets idle boards.
- One step at a time, port released between steps.
- Hardware sessions are batched: one desk sitting per milestone records every
  transcript, and then agents work for weeks with no board.
