---
status: fixed
found: 2026-08-01      # how: filetests (xtn.q32 dropped 849/849 -> 522/849 on the gate flip)
fixed: this change     # the host emulator now models flash; the image is firmware
area: lp-xt/lps-builtins-xt-app, lp-shader/lpvm-native (rt_emu/xt_image), lp-xt-emu
class: model-conflation   # presented as `capacity`; that was the symptom, not the class
related:
  - docs/defects/2026-08-01-xtensa-backend-cannot-select-float-constant-pool.md
---
# With `float-f32` in, the Xtensa builtins image fills the emulator's code region and leaves no room for shader code

**Symptom** — building the Xtensa builtins image with `--features float-f32`
succeeds, and then most of the Xtensa filetest suite fails to *link*:

```
xtn.q32   without float-f32:  6336/6336 tests, 849/849 files
xtn.q32   with    float-f32:  2917/6336 tests, 522/849 files   (327 files fail)
```

Failures report as `compile-fail`, not as wrong answers. Small shaders pass and
large ones fail, which is the tell.

**Cause** — it is capacity, not codegen. `lp-xt/lps-builtins-xt-app/link.ld`
gives `.text` 112 KiB (114,688 B) of the emulator's 128 KiB code region, and
`.rodata`/`.data`/`.bss` the remaining 16 KiB (16,384 B). Measured with
`xtensa-esp32s3-elf-size -A`:

| build | `.text` | free IRAM | `.rodata` | free DRAM |
| --- | --- | --- | --- | --- |
| default (Q32 only) | 66,300 B | 48,388 B | 12,176 B | 4,208 B |
| `--features float-f32` | 113,757 B | **931 B** | 16,156 B | **228 B** |

`rt_emu::xt_image::build_xt_image` places compiled shader code 4-aligned after
the image's `.text` and requires it to end before the image's data segments —
i.e. in exactly that free-IRAM gap. At 931 bytes, only the smallest shaders
fit; everything else fails with

```
compiled shader code does not fit the Xtensa code region: N bytes of shader
after 113757 bytes of builtins ...
```

The image *links* because 113,757 < 114,688. It links with 0.8% headroom, so
the f32 family very nearly does not fit its own region either.

**This is a different defect from the constant-pool one it was found behind.**
That one was "the f32 family does not compile for Xtensa"; it is worked around
and the image now builds. This one is "the f32 family does not *fit* alongside
shader code". Fixing the first exposed the second.

## Root cause — the capacity number was a symptom

Everything above is accurate and none of it is the cause. The cause is that
**the emulator modeled flash-resident firmware as if it lived in SRAM**.

On real ESP32 silicon the application's `.text` executes from flash through the
cache (XIP; boot logs show `vaddr=0x4200_0020 map` segments). Only code a JIT
produces at runtime lives in SRAM. The emulator had one modeled SRAM code
region and put both in it, so the builtins image and compiled shader code were
competing for bytes that on hardware they never share. "112 KiB of `.text`
leaves 931 bytes" was a true statement about a false map.

**Fix** — `BoardProfile` gained read-only IROM/DROM windows at each chip's real
flash-cache bases, plus an internal-SRAM region for an image's `.data`/`.bss`;
`lps-builtins-xt-app` links as flash-resident firmware; `rt_emu::xt_image`
places each segment by classifying its `p_vaddr`, and the compiled shader gets
the **whole** SRAM code region. The region did not grow — 128 KiB on the S3,
92 KiB on classic, unchanged — the builtins simply left it.

`--features float-f32` is now unconditional for the Xtensa builtins image;
`LP_XT_BUILTINS_F32` is gone.

**Regression coverage** — `scripts/filetests.sh -t xtn.q32` is back to 849/849
files (6336/6336 tests) *with* float-f32, and `xtlpn.q32` to 849/849
(6385/6385); `scripts/build-builtins-xt.sh` now asserts the image's segment
addresses, so a `link.ld` regression fails at the source rather than as a
loader error 300 files later; `lp-xt/lp-xt-emu/tests/call_range.rs` covers the
bug class below.

## The corrected classic-ESP32 conclusion

The reading this defect invited — "f32 doesn't fit on classic ESP32" — was
**wrong, and wrong in a way worth naming**: it conflated two unrelated budgets.

Classic's 92 KiB code region bounds **JIT'd shader code only**. It says nothing
about whether the f32 builtins fit, because on classic as on the S3 the
builtins are flash-resident (`irom_seg` at `0x400D_0000`, 3 MB). Classic f32
viability is therefore a **flash-budget** question plus an **unprobed LX6 FPU**
question, not an SRAM one — and the flash budget is the one with room to spare.

## Lesson

A capacity failure reported in the units of the thing that overflowed is
usually a modeling failure reported in the wrong units. The number (931 bytes)
was measured, reproducible, and led straight to three candidate resolutions —
widen the region, split the region, split the feature — **all three of which
would have preserved the wrong map**. The question that dissolved it was not
"how do we make it fit" but "why are these two things in the same region at
all, when on the device they are not".

The same model gap was also hiding a live bug class. A shader→builtin call
spans SRAM→flash, tens of megabytes on the S3 and far outside `call8`'s
±512 KiB — which is why the JIT patches indirect calls. In a single small
region an accidentally-direct call would have been *in range on the host* and
out of range on silicon: passing tests, failing hardware. It is now pinned in
`call_range.rs`, along with the finding that classic's IROM sits only ~192 KiB
above its SRAM1 I-bus window, so a direct call *would* work there. The emitter
must stay indirect because the S3 requires it, not because every Xtensa target
does.
