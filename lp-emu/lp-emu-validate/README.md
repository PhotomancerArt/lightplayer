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
lp-emu:esp32c6:t1               our machine, time grade 1 (M3)
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

When the payload also printed an in-band header (`[fw-checks-header] {…}`), the
two must agree; `Transcript::load` refuses them if they do not.

**Never edit a transcript.** A mismatch is a regression or a re-capture, never a
fixture to refresh. That is the FP replay rule, and it is the only reason a
committed capture means anything.

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
cargo run -p lp-cli -- validate run <set> --config <name> [--port …] [--dry-run]
cargo run -p lp-cli -- validate record <set> --config <name> --date … --commit … [--dry-run]
```

`--dry-run` prints the exact commands and stops, which is what makes a desk
protocol reviewable before a board is plugged in.

For silicon the runner does not reinvent the port discipline; it shells out to
`scripts/spike/esp-emu/desk-espflash-step.sh`, which runs espflash in the
**foreground** under `script(1)` with a `SIG_DFL` exec shim, polls for the
payload's sentinel, SIGINTs **that pid only**, and post-checks `lsof`/`pgrep`.
Every clause there is a sitting that broke.

For `esp-emu:*` it builds a merged image and runs the binary named by
`$LP_ESP_EMU` with `--exit-on` the payload's done marker (install per spike
report §10 — a checksum-verified release asset, outside the repo).

`lp-emu:*` is listed as `unavailable until M3`. The seam is
`driver::ConfigurationDriver`: implement it for the machine and nothing else in
this crate changes.

## Rules of the desk

- **Never open the port while Studio holds it.** `just hardware-list` is
  passive; `--probe` resets idle boards.
- One step at a time, port released between steps.
- Hardware sessions are batched: one desk sitting per milestone records every
  transcript, and then agents work for weeks with no board.
