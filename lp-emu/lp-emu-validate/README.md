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

- **`link`** — `UsbSerialJtag`, `Uart0Spike`, or (M5) `Uart0`. Until M6 every emulated run
  got the `spike_uart0_link` feature, which moves the host link onto UART0,
  because "neither emulator has a USB host". That stopped being true, and it
  mattered: comparing a UART0-link image on our machine with a USB-link image
  on the board compares two link drivers' allocations, not two machines
  (DD30). `esp-emu:*` still gets the workaround unconditionally — its USB
  model asserts SOF for ever (spike report §4) — and so do the two payloads
  whose committed transcripts are of that image. A transcript is never
  re-baselined to suit a later idea. `Uart0` is the third thing and is
  neither of the first two — see "`Link::Uart0` is not `Link::Uart0Spike`"
  below. On a chip other than the C6 this field is read off the payload's
  [`ChipArm`](#a-payload-on-a-chip-chiparm), not off the row.
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
silicon:esp32v3                 the desk's classic ESP32 (revision v3)
esp-emu:0.42.0                  Espressif's binary emulator, that version
lp-emu:esp32c6:t1               our machine, time grade 1 (instruction count)
lp-emu:esp32c6:t2               our machine, time grade 2 (per-class model)
lp-emu:esp32c6:t3               our machine, time grade 3 (+ what an address costs)
lp-emu:esp32v3:t1               our classic machine, and its ONLY grade
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

`lp-emu:esp32v3:t1` is the same rule applied to the classic, and its `memory`
row is the register for how a `because` is written: it names the **seven**
memory-class fields that are equal to the byte against the desk board on the
same bytes, **and the two that are not** — `[MEM] used` +84 B and the
`[stack]` high-water −960 B, both deterministic on both sides, with DD48's
carry to M4 P1 named. A `because` that listed only the wins is the thing this
table exists to prevent.

It also has **one** time grade, and that is a statement rather than an
omission: `TimeGrade` on `lp-emu-esp32v3` has one arm, there is no measured
LX6 per-class cost model, and a `t2` that was `t1` under another name would be
exactly the dishonesty the grades are for (M5 ruling R1). `ChipSpec::
time_grades` is where a second one would be added. Note also that the classic
has **no `usb-serial-jtag` row at all** — that part has no such peripheral —
and `validate list` still prints `usb-serial-jtag=modeled` for it, because a
class with no entry defaults to `modeled`; the absence of a row and a
`modeled` row are not distinguished in that view.

### A band: how wrong a class is allowed to be

A trust entry may also state a **band**, and one class needs it. A cycle model
is never exact, so a `--strict-timing` that compares for equality can never
let `timing` be anything but `modeled`, however good the model gets — a grade
that cannot move stops carrying information. A band is what a never-exact
class is graded against instead: an interval the per-sample ratio must fall
in, the fraction of a field's samples that must fall in it, an aggregate
tolerance the per-sample test cannot see, and — mandatory — the payloads that
measured it, because a band naming no payloads is a claim about payloads
nobody ran. The ratio is read reference-over-model, so a band means the same
thing whichever transcript is given to `replay` first, and a payload the `on`
list omits is compared exactly, as before. The field is additive: an entry
with no band behaves exactly as every entry did before it existed. See the
hardware-validation ADR's 2026-09-08 amendment, and
`docs/reports/2026-09-08-esp32c6-t3-calibration.md` §4 for the first one.

A grade and a band are separate claims: `--strict` reads the grade,
`--strict-timing` reads the band, and `lp-emu:esp32c6:t3` today states a band
at `documented` — which `--strict` refuses and `--strict-timing` enforces.

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

`baud` (added by M5 P1, additive in the same sense) is the line rate a capture
was taken at, and it is **also a discriminator in the filename**, like
`machine`. The classic prints its ROM and its second-stage bootloader at
115200 and its application at 921600, so one image at one commit on one date
is *two* transcripts that differ only in the rate the port was opened at, and
without a discriminator the second would overwrite the first. A capture with
no `baud` files exactly where it always did.

**The header is `deny_unknown_fields`, and it was earned.** It used not to be,
and four hand-written classic sidecars spelled three fields the way their
author remembered them — `board_mac` for `mac`, `chip_revision` for
`silicon_rev`, and `baud` before there was one. All three parsed cleanly and
were silently dropped: the classic's committed transcripts carried a MAC and a
chip revision that no code could see, with nothing to say so. A misspelled key
is now a loud refusal naming the field. (M5 P1 re-filed those four sidecars
onto the schema's keys — **keys only**, not one value changed and no `.txt`
touched; the filing test proves it, because `baud` reproduces the stems those
files already had.)

When the payload also printed an in-band header (`[fw-checks-header] {…}`), the
two must agree; `Transcript::load` refuses them if they do not.

**Never edit a transcript.** A mismatch is a regression or a re-capture, never a
fixture to refresh. That is the FP replay rule, and it is the only reason a
committed capture means anything.

**And never overwrite one** (M5 P4, DD51). The stem names the firmware and
the day and *not* the machine that ran it or the host script it ran — both of
which can change under one firmware commit, and both did when M5 P3
re-recorded M4's `upload-walk`. So the recorder refuses an existing stem, and
a second recording of one lands **beside** the first as `<stem>-r2.txt` (then
`-r3`, …), its sidecar and pin capture named the same way, with the sidecar's
`note` saying which recording it is. A suffix rather than the emulator's
commit in the name, because a silicon capture has no emulator and collides
the same way (two sittings on one commit in one day), the sidecar already
carries the tool versions, the source command and `firmware_sha256` — the
provenance a name could only abbreviate — and the stem stays what
`TranscriptHeader::file_stem` says, so nothing that names a transcript has to
learn a second grammar.

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

Whose the frame *count* is depends on the payload (`Payload::pin_capture`,
M5 P4). `rmt-chase` sends exactly 768 frames and parks, so a capture with a
different number is a broken recording (`PinCapture::EveryFrame`). A walk on
the shipped image — `shader-oracle-walk` — keeps rendering at the engine's
pace until the run ends on a console line, so how many frames the pad carried
by then is what the clock decided: the frames the two captures share are
compared as `Pin` and the counts are reported as `Timing` with their ratio
(`PinCapture::WhileRunning`). A payload with no per-frame record of its own
has no guest claim for its pad to disagree with; what its frames are held
against is the other transcript and, for the oracle walk, the host oracle's
line in `tests/m5_replays.rs`.

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
* `--strict` refuses any class either side grades below `measured`;
* `--strict-timing` fails a timing difference. Where a side states a **band**
  for this payload it fails only *outside* the band — per-sample coverage and
  the aggregate, both — and names the band, the observed coverage and the
  observed aggregate so a reader can see how far outside it landed. Where
  neither side states one, it is the exact comparison it always was. Either
  way the ratios are still printed: reading a number and gating on it are
  different acts, and PD9/D13 stands — no host gate runs on emulated
  microseconds.

Masking (`src/mask.rs`, the `scripts/spike/esp-emu/mask-transcript.sh` rules as
code) names the differences that mean nothing — heap digits in prose, uptime,
fps, tick counters, bootloader timestamps — each with the reason it is ignored.
Masking here means *reported but not compared*, never *deleted*.

## The chip table (M5 P1)

The runner drives more than one chip, and everything that differs between
them is **one row** in `ChipSpec` (`src/driver.rs`) — crate directory, target
triple, profile, binary name, partition table, flash size, espflash's own
chip word, the emulator package, the vendored mask ROM's stem, the monitor
baud, the serial bridge's port prefix, and the time grades the machine
actually defines.

| | `esp32c6` | `esp32v3` |
|---|---|---|
| `espflash --chip` | `esp32c6` | **`esp32`** — espflash does not know the revision |
| chip cargo feature | `esp32c6` | `esp32` |
| target | `riscv32imac-unknown-none-elf` | `xtensa-esp32-none-elf` |
| profile | `release-esp32` | `release-esp32v3` |
| firmware crate | `lp-fw/fw-esp32c6` | `lp-fw/fw-esp32v3` |
| emulator package | `lp-emu-esp32c6` | `lp-emu-esp32v3` |
| mask ROM | `esp32c6_rev0_rom.elf` | `esp32_rev300_rom.elf` |
| `--monitor-baud` | none | **921600, not optional** |
| port | `cu.usbmodem…` (native USB) | `cu.wchusbserial…` (CH340K) |
| time grades | `t1` `t2` `t3` | `t1` — and only `t1` |
| product link | USB-Serial-JTAG | **UART0** (no USB-SJ peripheral) |

**The table is a mirror, and the mirror is the contract.** Its values are the
`justfile`'s (`xt_v3_target`, `v3_flash_size`, `fw_esp32v3_dir`, the profiles,
`flash-fw-esp32v3`) and `scripts/emu/build-reference-image.sh`'s `case`. A
value that drifts here builds a different image than the one a human builds by
hand, and two transcripts of "the same" image would not be comparable — which
is the only thing this crate exists to make them. Two tests check the mirror
against the script on every `cargo test`, and the second of them found real
drift the day it was written (`render-basic` and `render-rocaille` had been in
the script and not in `reference_image_slug` since the render benches landed).

Two entries in the table are the ones a reader will otherwise get wrong.
`--monitor-baud 921600` on the classic is **not optional**: `board::esp32v3::
init` reprograms `clkdiv` mid-stream, so the ROM and the second-stage
bootloader talk at 115200 and the application at 921600, and a monitor left at
115200 reads the application as line noise. One image therefore produces
**two** transcripts, which is what the header's `baud` is for. And the
held-port pre-check was a literal `usbmodem` grep until M5, so a held CH340
port passed it silently; `ensure_port_free` takes the prefix from this table
*and* checks the request's own `--port`.

Adding a chip is one row plus arms on the payloads that run there. M6's S3 is
the next one, and that is the shape it should take.

### A payload on a chip: `ChipArm`

A `Payload` had no chip until M5, because until M5 there was one chip. Three
of its fields turned out to be chip facts — `firmware_features`, `link`,
`host_plan` — and one thing the C6's `boot-idle` does not need turned out to
be mandatory on the classic: a `host_script`.

Everything else is chip-independent **by construction**: `fw_checks_feature`,
`emits_header`, `sentinel`, `mask_set`, `fields`, `series`, `record_kinds`.
Both chips print the same `[MEM] free=` / `[JIT] used=` / `[stack]
heartbeat:` lines out of the same `lpa-server` / `fw-core` code. That is what
makes **one payload name over two chips** possible, and it is why the classic
gets arms rather than `v3-`-prefixed payload names: the committed silicon
transcript's sidecar already says `"payload": "boot-idle"`, and transcripts
are never edited.

Read an arm through `Payload::arm(chip)`. `esp32c6` is *synthesised* from the
top-level fields rather than duplicated, so a row that says nothing about
chips means exactly what it always meant. A payload with no arm for the
requested chip is a refusal that names the chips it does run on — never a
fallback to the C6's features on another chip, which would build an image that
does not exist.

One arm field is a fact that cost a bench sitting to find. **`second_boot`**:
espflash hard-resets after writing, so every silicon capture of the classic is
the boot *after* the one that formatted `lpfs`. A machine handed a fresh flash
copy is on its **first** boot and reports `largest_free=106494` — 2032 bytes
short — for a reason that has nothing to do with the model. (The C6's
`fresh_chip` says very nearly the opposite thing, and the two are not
interchangeable.)

### `Link::Uart0` is not `Link::Uart0Spike`

`Uart0Spike` is a **C6 workaround**: the cargo feature `spike_uart0_link`
moves the host link onto UART0 so an emulator with no USB host can be served.
`Uart0` is the classic's **product** link — that part has no USB-Serial-JTAG
peripheral at all. Conflating them would put a feature that does not exist on
a classic build (an unbuildable command line) and would write `uart0-spike`
into every classic sidecar, where it reads as a workaround rather than as the
product.

## Runner

```bash
cargo run -p lp-cli -- validate list
cargo run -p lp-cli -- validate replay <transcript> --against <transcript|configuration>
cargo run -p lp-cli -- validate run <set> --config <name> [--port …] [--image …] [--link real|spike] [--dry-run]
cargo run -p lp-cli -- validate record <set> --config <name> --date … --commit … [--link real|spike] [--dry-run]
```

`--dry-run` prints the exact commands and stops, which is what makes a desk
protocol reviewable before a board is plugged in.

`--link real|spike` (`lp-emu:*` only; M1 P1, DD8) forces a payload's effective
link for this one run, overriding its registry [`Link`](src/payload.rs) —
`real` takes the shipped USB-Serial-JTAG path (no `spike_uart0_link`), `spike`
takes the UART0 workaround. It exists for payloads such as
`shader-compile-stress`, whose `link` field stays `Uart0Spike` in the registry
because three committed transcripts are of that image and moving the field
would invalidate them, but which can still be *recorded* over the real link
into a **new stem** for a like-for-like comparison against a silicon capture
that used it. The override never touches `validate.toml` or the payload row;
the sidecar's `firmware_features` line (present/absent `spike_uart0_link`) and
its `note` (present only when the override actually changed something) are how
a reader of the transcript alone tells it apart from a run of the payload's
own default link.

A payload that carries no `host_plan` — every row written for `Uart0Spike`,
which has no host to schedule — gets one synthesised when `--link real` moves
it onto `UsbSerialJtag`: `attached`, no script, the same state
`espflash --monitor` puts a board in from its first byte. Without it the run
builds and executes but nothing ever drains the wire, so the payload's own
sentinel never reaches the capture and nothing is recorded. A payload that
already carries its own `host_plan` is unaffected — the override changes the
link, never an application's own choice of when a host reads it.

For silicon the runner does not reinvent the port discipline; it shells out to
`scripts/emu/desk-espflash-step.sh`, which runs espflash in the
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
