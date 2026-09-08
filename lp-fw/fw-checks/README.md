# fw-checks

`fw-checks` is **the payload crate** for LightPlayer's hardware-validation
system (vision D14): a payload is a module here behind a cargo feature, many
per image, and the host side that runs and replays them is
`lp-emu/lp-emu-validate` (`lp-cli validate`). Read that crate's README for
what a payload, a configuration, a transcript and a replay are.

A payload answers one focused question on a real chip: does a feature work,
how much memory does a workload use, how fast is a firmware path. Payloads sit
between unit tests, filetests, profiles and demos — small, named scenarios
with known features, markers and structured records.

The crate is `no_std` by default so firmware crates can depend on it cheaply.
Enable `std` for host-side reporting used by `lp-cli fwcheck`.

**Keep it cheap.** The whole point of this crate is that a firmware image can
depend on it without dragging anything in. A payload whose portable half would
need the product's wire types does not belong here — `test_json`'s heartbeat
is the worked example, and it stays a shipped-image scenario instead.

## What a payload prints

```text
[fw-checks-header] {"schema":1,"payload":"…","chip":"…","firmware_commit":"…", …}
[fw-check-json] {"kind":"case-summary", …}
… human log lines the host parses as indexed series …
[<payload>] === DONE ===          (or a readiness line, for payloads that serve)
```

The **header** goes first, before any record. `emit_header` writes it `no_std`
with no dependency beyond `log`; the JSON is hand-written so a payload with no
other reason to pull in `alloc` does not gain one. It carries only what the
firmware knows about itself — payload, chip, commit, feature set — and every
field comes from a `build.rs` `env!`, so it cannot drift from the binary it
describes. Everything the *host* knows goes in the transcript's `.meta.json`
sidecar, and `lp-emu-validate` refuses a transcript whose two halves disagree.

`fw-esp32c6`'s `build.rs` provides `LP_BUILD_COMMIT`, `LP_BUILD_DIRTY` and
`LP_BUILD_FEATURES` for exactly this.

**Records** go through `emit_record_json`, behind `[fw-check-json] `. Keep
them JSON objects with a `kind`.

## Adding a payload

1. Add a `check-<name>` feature and a module under `src/checks/<name>/` holding
   everything that is arithmetic over bytes — protocols, state machines,
   record types. That half is `no_std` and unit-tested on the host.
2. Add the `FwCheck` variant and the `ALL_CHECKS` entry (feature name, done
   marker, trace slug, supported targets).
3. Point the firmware feature at
   `["dep:fw-checks", "fw-checks/check-<name>"]`, so the old
   `just fwtest-<name>-*` recipe keeps working.
4. Write the firmware harness: board init, peripherals, the clock, the header,
   and delegation to the shared module for everything else.
5. Add the matching `Payload` to `lp-emu-validate`'s registry — the field
   classes and series patterns that make a transcript replayable. It mirrors
   this crate rather than importing it (the `lp-emu/` MIT fence), and
   `lp-cli/tests/validate_registry_parity.rs` is what keeps the two honest.

A payload that has **no module** — a shipped-image walk, where the product
image itself is the subject — skips 1 to 4 and does only 5, plus an
`ALL_CHECKS` entry naming the image's own features with `emits_header: false`.
See `boot-idle` below.

**Do not move a wire format in the same change as a refactor.** `lp-cli`
parses these lines, and a protocol change hidden inside a migration is how a
desk session gets wasted.

## The payloads

| payload | feature | sentinel | shared module |
|---|---|---|---|
| `shader-compile-stress` | `test_shader_compile_incremental` | `[inc-shader-compile] === DONE ===` | `checks::shader_compile` (record types, host reporter) |
| `gpio-calibrate` | `test_gpio_calibrate` | `CAL READY target=` (it serves; it never finishes) | `checks::gpio_calibrate` (the `CAL` line protocol, the duty ramp) |
| `uart-bridge` | `test_uart_bridge` | `UART-BRIDGE READY ` (it serves until unplugged) | `checks::uart_bridge` (the bounded queue, the pump step, the ready line) |
| `jit-math-perf` | `test_jit_math_perf` | `[jit-math-perf] === DONE ===` | `checks::jit_math_perf` (the corpus, the Q32 kernels, the benchmark runner — the cycle counter itself is injected as a `fn() -> u32`, since reading it is a chip fact rather than portable arithmetic) |
| `rmt-chase` | `test_rmt`, `ws281x_telemetry` | `[rmt-chase] === DONE ===` | `checks::rmt_chase` (the chase pattern, the FNV-1a checksum, the per-frame record) |
| `boot-idle` | *(none — the shipped image)* | `[stack] heartbeat: high-water` | *(none)* |
| `usb-negative-control` | *(none — the shipped image)* | `"hostDrainingAgainMs"` (the recovery stamp itself) | *(none)* |
| `usb-detach-reattach` | *(none — the shipped image)* | `"uptime_ms":10000` (a whole heartbeat after the re-open) | *(none)* |
| `usb-host-absent` | *(none — the shipped image)* | *(none — it prints nothing; see below)* | *(none)* |

### `boot-idle` is the shipped image, not a module

The other odd one out, and the reason two fields on `FwCheckConfig` exist.
`boot-idle` is the product image built `server,radio,memory_fs` on top of the
defaults, run to its first idle stack heartbeat — a **shipped-image walk**
(vision Q1), which is a scenario kind rather than a check. There is no
`check-boot-idle` feature and no `src/checks/boot_idle/`, because there is no
arithmetic over bytes to share: what it prints is what the firmware prints on
any boot.

So `firmware_features` is a list here rather than one `test_*` switch, and
`emits_header` is `false` — nothing in the image calls `write_header`, so the
transcript's `.meta.json` sidecar is the whole provenance. Steps 1–4 of the
recipe above do not apply to a payload like this; step 5 does, and it is the
only step it needs.

(`esp-emu:*` adds `spike_uart0_link` on top, which moves the host link to
UART0, because that emulator's USB model asserts SOF for ever and the
firmware would serve into the void believing a host was there. Our own C6
machine models the host, so since M6 it runs the shipped image on the shipped
link — the link is a property of the payload, in the host-side registry.)

### The three USB scenarios are the same image asked about its link

`usb-detach-reattach` and `usb-host-absent` join `usb-negative-control` as
shipped-image scenarios with no module: what differs between them is not the
firmware, it is what the **host** does. So the difference lives in the
host-side registry (`Payload::host_plan`, `Payload::capture`), and this side
carries only the sentinel each one can actually reach.

Two of those sentinels are worth the sentence. `usb-negative-control` cannot
use a stack heartbeat: `stack_probe` reports only when the high-water mark
has grown, so the one report it could have seen went into a closed port and
there may never be another — measured on the emulator twin, one `[stack]`
line in a twenty-second run, at five seconds, into the dark. And
`usb-host-absent` has no marker at all, because with no cable the device says
nothing: its transcript is the emulator reading statics out of the guest, and
only an emulator can record it.

### `usb-negative-control` is the same image, watched differently

Two payloads share a sentinel and nearly a feature list, and the difference
between them is not in this crate at all.

`usb-negative-control` is the shipped image built `server,radio` — flash-backed,
the product's own bytes — and what makes it a distinct payload is **when the
host side opens the port**. `boot-idle` is flashed with `espflash --monitor`
and watched from its first byte. This one is flashed with no monitor at all,
left alone for several seconds, and only then read by a non-resetting reader.

That is the only way to observe the state the firmware cannot report on while
it is in it: a host attached (SOF arriving) with nobody draining the port.
Every protocol write times out, the connection monitor latches, and the log
line saying so is dropped by the latch that emitted it — the outgoing queue is
gated on `is_connected()`. What survives is a pair of timestamps on the
device's own clock, in the next heartbeat's `link` object
(`hostNotDrainingMs`, `hostDrainingAgainMs`, `notDrainingCount`; M6 P1b).

The registry that carries that difference is the **host's**
(`lp-emu-validate`'s `Payload::capture`), not this one. `FwCheckConfig`
describes what the firmware is and prints; when the operator opens the port is
a fact about the operator.

### `rmt-chase` is the first payload whose claim is checked off a pin

Every other payload here is believed because the device said so. This one
prints what the driver *thinks* it sent — one `rmt-frame` record per frame,
with an FNV-1a checksum over the RGB bytes — and the emulator reads the same
frame back off **the pad**, decoding the WS281x waveform the RMT actually put
on GPIO18. The gate is that the two checksums agree, frame by frame. That is
why its record fields are graded `Structural` while the `[WS281X]` telemetry's
counters are graded `Pin`: `frames`, `complete`, `trips`, `refills` are claims
about what reached the wire, and the wire is now observable.

Its pattern is a white dot (`[10, 10, 10]`) on black, which is invariant under
any permutation of the three colour channels — deliberately, because
`LedChannel` swaps RGB to GRB and `lp-ws281x` then permutes again, so the
bytes on the wire are the caller's RGB unswapped. That double swap is a
finding filed against the harness path (DD34 d), not something this phase
fixed, and the payload is built so that settling it later cannot invalidate a
committed transcript.

### `uart-bridge` is an instrument, not a measurement

It is the odd one out and worth a paragraph. The other payloads answer a
question about the board they run on; this one turns a **spare** board into the
lab's USB-to-UART tap, so that some *other* board's UART0 console reaches the
Mac. There is no adapter on this bench, and the one behaviour esp-emu can never
produce — a firmware declaring its USB host undrained — is logged over the very
link it declares undrained. A second board is the way out of that circle.

Three of its properties are contract, not implementation:

- **It installs no logger.** With no `log` sink registered, every `log::` call
  in the image is a no-op, so an esp-hal `debug!` cannot appear in the middle of
  somebody's boot capture. Its two boot lines go through `esp_println` instead,
  and the header line is byte-identical to `emit_header`'s because it uses the
  same `Display`.
- **After those two lines it is silent forever.** A host reading its port sees
  the other board's bytes and nothing else.
- **Byte losses are reported at the NEXT boot, never mid-stream.** A bridge that
  announced a drop would be corrupting the capture at the moment the capture got
  interesting. The counts live in RTC fast memory, so a reset preserves them and
  a power cycle clears them; `prev_drop_to_uart=0 prev_drop_to_usb=0` means the
  run before this reset was clean, and `4294967295` means UART0's hardware RX
  FIFO overran and the bridge cannot know by how much. **A transcript captured
  through this bridge is only worth reading while both are zero.**

Default 115,200 8N1 — the mask ROM's rate, which is what a boot banner arrives
at whatever the far side's driver does later. `uart_bridge_fast` selects 921,600
for a far side running `spike_uart0_link`'s driver instead.

Flashing and wiring live in `scripts/emu/` (`uart-bridge-flash.sh`,
`uart-bridge-wiring-check.sh`, `board-port.py`, `tty-capture.py`), never in a
chat message, and they name the board by MAC because two XIAO C6s are
indistinguishable to a port list.

The other `check-*` features are declared and have no module yet; the Q12
ledger in the 2026-09-06 esp-emulator plan's `notes.md` tracks which `test_*`
harness becomes which.

## Running one

```bash
cargo run -p lp-cli -- validate list
cargo run -p lp-cli -- validate run <set> --config <name> --port … --dry-run
cargo run -p lp-cli -- validate record <set> --config <name> --commit …
```

`lp-cli fwcheck` is the older, single-check front door and still works:

```bash
cargo run -p lp-cli -- fwcheck list
cargo run -p lp-cli -- fwcheck run esp32c6 shader-compile-stress --note baseline
```

Its hardware runs write a timestamped directory under `traces/` with
`trace.txt`, `records.jsonl` and `report.txt`. `validate record` writes into
the committed transcript tree instead, with the provenance header filled in.
