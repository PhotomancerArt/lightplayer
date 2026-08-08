# lp-xt-fp-harness

The Xtensa FP conformance rig. It runs [`lp-xt-fp-vectors`](../lp-xt-fp-vectors)'
corpus on the FPU of whatever board it is flashed to and prints the answers over
the serial monitor, one `[FPCONF]` line at a time.

## It decides nothing

There is no PASS, no FAIL, and no comparison against a golden anywhere in this
crate. The goldens live on the host, in `lp-xt-emu/tests/fixtures/fp/`, and were
committed **before any hardware ran**; classification is `just fp-diff`'s job.

That split is the whole design. A device that decided for itself whether it
agreed would be a tautology — it would pass forever, including on the day the
silicon changed. For the same reason, **a device disagreement is a finding to
triage, never a reason to edit a golden.**

## One rig, two boards

Chip identity and build provenance arrive as `BoardId` from the firmware. The
crate holds no board knowledge of its own, which is what lets one copy serve
both Xtensa targets:

| firmware | chip | recipe |
|---|---|---|
| `fw-esp32s3` | ESP32-S3 (LX7) | `just fwtest-xt-fp-esp32s3 <port> [family\|tables\|helpers]` |
| `fw-esp32v3` | classic ESP32 (LX6) | `just fwtest-xt-fp-esp32v3 <port> [family\|tables\|helpers]` |

It was a private module inside `fw-esp32s3` until the classic needed it too.
Copying it was rejected on the grounds that it is a **correctness oracle**: two
copies that drifted would leave us diffing two chips against subtly different
vector sets while both looked healthy — exactly the failure the fingerprint
abort exists to prevent between host and device, reintroduced between firmwares.

## Calling it

```rust
lp_xt_fp_harness::run_all(lp_xt_fp_harness::BoardId {
    chip: "esp32s3",
    build_commit: env!("LP_BUILD_COMMIT"),
    build_dirty: env!("LP_BUILD_DIRTY"),
    build_profile: env!("LP_BUILD_PROFILE"),
})
```

The `env!` calls belong at the **call site**, not in here. `env!` expands in the
crate that names it, so a build stamp read inside this crate would describe this
crate's compilation instead of the firmware's.

## Modes

Selected at build time through `LP_FP_MODE` / `LP_FP_FAMILY` / `LP_FP_LIMIT`,
because the harness has no input channel — it prints and never reads.

- **`families`** — the six conformance families, diffable against committed host
  predictions. This is the mode with an oracle.
- **`tables`** — exhaustive sweeps of the implementation-defined estimate ROMs
  behind `recip0.s` / `rsqrt0.s` / `sqrt0.s` / `div0.s`.
- **`helpers`** — the divide-step probe grids the emulator's model was fitted
  against.

`tables` and `helpers` are **silicon-first**: there is nothing to predict, which
is why they have to be read off hardware. With a second chip in the picture, one
board's committed capture becomes the other's oracle.

⚠️ This crate's `build.rs` is what makes those switches take effect. Cargo does
not track `option_env!` on its own, so without it, changing the family reuses the
previous build and the board runs the wrong subset while looking fine.

## Why the FP instructions are `global_asm!`

The device must execute specific instructions on specific operands, which is not
something the Rust compiler can be asked for. The kernels are hand-written asm —
about fifteen of them, one per operation shape — and every vector is a *call*
into one, not a compiled program of its own. That keeps the kernel count at
fifteen instead of 5,630 and means the campaign needs no FP emitter.

The `<div-sequence>` and `<sqrt-sequence>` pseudo-ops run the toolchain's real
`__divsf3` and `sqrtf` sequences, transcribed from objdump output under the
AGENTS.md license rule (disassembly is fact; no library source was read). Both
are byte-identical across the `esp32` and `esp32s3` multilibs of esp-14.2.0,
verified 2026-08-06 — that equality is what makes one transcription valid for
both boards, and it should be re-checked before a third chip is added.
