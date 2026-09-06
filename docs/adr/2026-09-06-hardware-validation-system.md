# One hardware-validation system: payload, configuration, transcript, replay

- **Date:** 2026-09-06
- **Status:** accepted
- **Context:** M2 of the 2026-09-06 esp-emulator plan (PD3, PD4; vision D10,
  D14). PR #533.
- **Supersedes:** nothing. It composes five existing mechanisms rather than
  replacing them; the two it will retire are named below.

## Context

Five mechanisms answered "does this work on the chip", and each got one thing
right:

| mechanism | what it got right |
|---|---|
| `lp-emu/lp-xt-emu/tests/fp_silicon_replay.rs` | verbatim silicon captures as committed fixtures, replayed every test run, **never edit a capture** |
| `lp-fw/fw-checks` + `lp-cli fwcheck` | named checks as a `no_std` crate, a config table, structured JSON records |
| `lp-xt/lp-xt-fp-harness` | the same generator crate on host and device; drift is a hard stop |
| `scripts/device-scenarios/` | captures with an `expect` list, and a port-holding runner whose every rule is a sitting that broke |
| ≈20 `test_*` cargo features + `just fwtest-*` | coverage — at one full image rebuild and reflash per payload |

What none of them had was a way to say *what a number is worth*. The
2026-09-06 esp-emu spike made that concrete and urgent: on one image, in one
run, Espressif's emulator was **byte-equal to silicon on every memory field**
(peak/resident/after-drop 48,132 / 18,932 / 3,976 B and all 184 per-tick
values) and **2.37x wrong on time**, with the compile harness's own 5 ms slice
budget *passing* under the emulator and *failing* on the board. Its
USB-Serial-JTAG model asserts SOF forever and reports EP1 free forever, so the
shipped firmware serves into the void believing a host is attached — and
nothing in its output says so.

A system that treats "esp-emu said 48,132" and "esp-emu said 4,625 µs" as the
same kind of fact will eventually green-light a build the desk rejects.

## Decision

**One system, four nouns and one verb.** Host side:
`lp-emu/lp-emu-validate`, behind `lp-cli validate`. Device side:
`lp-fw/fw-checks`, extended.

### Payload

A module in `fw-checks` behind a cargo feature, runnable many per image
(vision D14). It prints a header line, then records behind
`[fw-check-json] `, then human log lines the host parses as indexed *series*.

The old `test_*` firmware feature survives as an alias pointing at
`["dep:fw-checks", "fw-checks/check-<name>"]`, so existing `just fwtest-*`
recipes keep working while the payload becomes the real thing.

### Configuration

A named value, not a mood (plan PD4):

```text
silicon:seeed/xiao-esp32-c6
esp-emu:0.42.0
lp-emu:esp32c6:t1
```

It appears in the transcript header, in the runner's table, and in the
transcript's filename. Configurations that do not exist yet are **listed as
unavailable** rather than omitted — `lp-emu:*` says `unavailable until M3` —
so the seam has a name before it has an implementation.

### Trust is per field class, stated, with a reason

Every comparable field belongs to a class: `memory`, `timing`, `pin`,
`usb-serial-jtag`, `boot-log`, `wire`, `structural`. Each configuration's
entry in `validate.toml` grades it per class as
`measured | documented | modeled`, each with a `because` that cites the
measurement. **A class with no entry is `modeled`: silence is not trust.**

This is the ADR's load-bearing decision. It is what lets one transcript be
simultaneously authoritative about heap and worthless about microseconds,
which is exactly what the spike found and what a single per-configuration
trust flag could not express.

### Transcript

Committed, verbatim:

```text
lp-emu/transcripts/<chip>/<payload>/<configuration>-<date>-<short-commit>.txt
lp-emu/transcripts/<chip>/<payload>/<configuration>-<date>-<short-commit>.txt.meta.json
```

The `.txt` is the bytes as captured — ANSI escapes, espflash progress bars,
ROM banners, carriage returns. A `.gitattributes` marks the tree `-text`
because the repository sets `core.autocrlf = input`, and a "verbatim" capture
git edited on the way in is not a capture.

The header has **two halves, because two parties know two different things**:

- the **in-band** line (`[fw-checks-header] {…}`) is printed by the device and
  carries only what the firmware knows about itself — payload, chip, commit,
  feature set — every field from a `build.rs` `env!`, so it cannot drift from
  the binary;
- the **sidecar** is written by the runner and adds what only the host knows:
  the configuration, board id, MAC, silicon revision, tool versions, capture
  method, and the trust table.

The sidecar is **required and authoritative**; the in-band line is optional
(the two transcripts this system was built on predate it) but must agree when
present. A transcript with no provenance is not a transcript.

**Never edit a transcript.** A mismatch is a regression or a re-capture, never
a fixture to refresh. That is the FP replay rule, adopted wholesale.

### Replay

Field by field, each comparison carrying its class:

- a difference in **memory**, **pin**, **wire**, **usb-serial-jtag** or
  **structural** fails;
- a difference in **timing** or **boot-log** is *reported with its ratio*, not
  failed — time is where a configuration is allowed to be wrong, and hiding
  that would be the mistake (plan PD9: no host gate on emulated microseconds);
- `--strict` refuses any class either side grades below `measured`.

Masking (`src/mask.rs`) is `scripts/spike/esp-emu/mask-transcript.sh` turned
into code, each rule carrying its class and the reason the difference means
nothing. **Masking means reported-but-not-compared, never deleted.**

### Runner

`list`, `replay`, `run`, `record`. Drivers **plan first and execute second**,
so `--dry-run` prints the exact commands — which is what makes a desk protocol
reviewable before a board is plugged in. The silicon driver shells out to
`scripts/spike/esp-emu/desk-espflash-step.sh` rather than re-deriving the port
discipline: foreground espflash under `script(1)` with a `SIG_DFL` exec shim,
sentinel poll, SIGINT to that pid only, `lsof`/`pgrep` post-check. Every clause
there is a sitting that broke.

## Alternatives considered

**Extend `lp-cli fwcheck` in place.** It already builds, flashes, captures and
reports one check. Rejected: it has no notion of a configuration, and its
trace directories are timestamped scratch rather than committed evidence. The
missing half was never the running; it was knowing what a captured number is
worth. `fwcheck` stays as the single-check front door.

**Put the payload registry in `lp-emu-validate` and have `fw-checks` import
it.** Rejected by the fence: `lp-emu/` is MIT as a unit and must not depend on
AGPL product crates, and `fw-checks` cannot depend on an MIT crate that would
then need to stay `no_std` for the device. The registry is therefore
**mirrored**, and `lp-cli` — which depends on both — owns the parity test
(`lp-cli/tests/validate_registry_parity.rs`). Duplication that nothing checks
is duplication that drifts; duplication with a test is a boundary.

**One trust grade per configuration.** Simpler, and wrong in exactly the case
that motivated the work: esp-emu is `measured` for memory and `modeled` for
time in the same transcript.

**Fail on timing divergence by default.** Rejected. Every cross-configuration
replay would be red, and a permanently red gate is a gate nobody reads. The
ratio is printed instead, and PD9 keeps host gates off emulated microseconds.

**Normalise transcripts on the way in** (strip ANSI, drop CRs, mask heap
digits) so files diff cleanly. Rejected: it makes the committed artifact a
derivative of a capture nobody kept, and the normalisation rules then cannot
change without invalidating history. Masking happens per comparison instead,
so the same file can be read under different rules forever.

## Consequences

- Every claim about a chip now has a place to live and a grade attached to it.
  `lp-cli validate list` prints the grades.
- Spike report §11.1 is no longer a table a human read: nine assertions
  reproduce it on every `cargo test`, with three negative controls (a
  corrupted `peak_used` digit, a corrupted per-tick heap value that no summary
  would catch, and a missing sentinel).
- Adding a payload is a five-step recipe in `lp-fw/fw-checks/README.md`, and
  the last step — registering it host-side — is enforced by a test.
- M3 implements one trait (`driver::ConfigurationDriver`) and the whole system
  starts working against our own machine.
- The `test_*` sprawl retires payload by payload, tracked in the plan's Q12
  ledger. `test_gpio_calibrate` went first; `test_json` deliberately did not —
  its portable half is a `WireServerMessage`, and dragging the product's wire
  types into the check crate is the wrong direction. It becomes a
  shipped-image scenario instead.
- **Open:** the timed USB-Serial-JTAG negative control (plan G3) needs a
  firmware build this repository does not have — the USB-SJ host link
  unchanged, the log sink also on UART0. `spike_uart0_link` does the opposite.
  Recorded in `g3-desk-batch.md` and assigned to M6.
