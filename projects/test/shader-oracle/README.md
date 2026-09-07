# shader-oracle

A project whose only job is to render **the same bytes everywhere**, so a
device's output can be diffed against a host's instead of admired.

It is the fixture for `lp-app/lpa-server/tests/shader_oracle_frame.rs` and
`scripts/m4-hardware-walk.sh`. It is not a demo — it is deliberately plain to
look at and deliberately awkward to change.

## The four constraints, and what breaks if you relax one

**No clock, and no `time` input.** Every frame is identical, so a host render
and a device render can be compared without synchronising anything. Add a time
input and the comparison needs the two sides to agree on *when*, which they
never will.

**Exactly 64 LEDs.** The device-side readout caps its one-shot hex dump at
`MAX_DUMP_LEDS` — 64, in
`lp-fw/fw-esp32s3/src/output/rmt/frame_dump.rs`. At exactly 64 the dump is the
*whole* frame, so the comparison covers every byte. Make the fixture bigger and
the diff silently becomes a prefix check — passing while the untested tail
diverges. The host test transcribes the same constant, so raising the fixture
means raising both.

**A neutral output pipeline.** `output.json` sets `white_point [1,1,1]`,
`brightness 1`, and LUT, dithering and interpolation all off. Under exactly that
configuration `DisplayPipeline` collapses to a stateless `(v + 0x80) >> 8`, so
the bytes on the wire are the engine's samples and nothing else. Turn dithering
or interpolation on and the pipeline becomes *temporal*: the same input yields
different bytes depending on frame history, and the oracle stops describing the
device.

**A shader that is hard to get accidentally right.** `shader.glsl` reaches for a
divide, a square root, a two-argument arctan, transcendentals through the
builtin table, a data-dependent branch, and a helper call. A pattern of flat
ramps would render plausibly through a half-broken backend; this one does not.

## Running it

```bash
cargo test -p lpa-server --test shader_oracle_frame -- --nocapture
scripts/m4-hardware-walk.sh            # flash an ESP32-S3, push, render, compare
```

The walk flashes `just flash-fw-esp32s3 <port> frame-dump`. The RMT driver now
drives real LEDs, and an LED cannot be diffed; `frame-dump` is the opt-in
feature that also prints each transmitted frame as `[OUT] dump …` / `[OUT]
frame …`. A default build renders exactly the same bytes and says nothing about
them, so the walk would report "nothing rendered".

The test prints two transcripts. `[ORACLE]` is wasmtime; `[ORACLE-RV32]` is
`lpvm-native`'s rv32 emulation — the same code generator firmware JITs, one ISA
over. Which of the two a device agrees with is the diagnosis, not a detail:
see `docs/defects/2026-07-30-q32-native-vs-wasmtime-last-bit.md`, where the
distinction was the difference between "the Xtensa backend is broken" and "the
native and WASM engines have always differed in the last bit".

## Known-good

2026-07-30, ESP32-S3 (XIAO ESP32-S3 Plus, rev v0.2), `fw-esp32s3` with
`node-shader` + `node-fixture`: **192 of 192 bytes identical** to both host
engines, `crc=0x55772254`, compiled on device in 63 ms.
