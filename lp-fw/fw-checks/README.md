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
