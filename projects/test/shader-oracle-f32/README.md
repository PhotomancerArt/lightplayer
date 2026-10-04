# shader-oracle-f32

`shader-oracle` with its shader pinned to **Float** (`"float_mode": "float"`
in `shader.json`): native IEEE-754 f32 on a board with an FPU (the ESP32-S3),
a compile error on a build that linked no f32 backend (the C6). Everything
else — the time-invariant shader, the 64-LED fixture on `ws281x:local:D10`,
the neutral output pipeline — is `shader-oracle`'s, byte for byte; its README
says why each of those constraints is there.

## What it is for

It checks that **f32 shaders survive preemption**: that the FPU registers a
render is using come back intact when another esp-rtos thread (the S3's link
thread, `lp-fw/fw-esp32s3/src/io_thread.rs`) interrupts it.

The host oracle cannot render Float (the wasm CPU preview refuses it), so
the oracle is **self-consistency**: the shader reads no clock, so every frame
after the first must be byte-identical, with the link quiet and while
`lp-cli link rtt`'s transfers and requests run:

```bash
lp-cli link rtt emu:<fw-esp32s3 ELF> --chip esp32s3 \
    --project projects/test/shader-oracle-f32 --json f32.json
# f32.json: every emu.ws281x[] entry has wire_distinct_after_first == 1
# and an empty wire_changes_after_first_us
```
