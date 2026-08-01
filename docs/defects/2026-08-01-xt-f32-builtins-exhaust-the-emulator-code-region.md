---
status: open           # blocks making float-f32 the Xtensa image's default
found: 2026-08-01      # how: filetests (xtn.q32 dropped 849/849 -> 522/849 on the gate flip)
area: lp-xt/lps-builtins-xt-app, lp-shader/lpvm-native (rt_emu/xt_image), lp-xt-emu
class: capacity
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

## Consequence

`scripts/build-builtins-xt.sh` cannot make `--features float-f32` its default:
`scripts/filetests.sh` builds this image before running `xtn.*`/`xtlpn.*`, so
the flip takes the whole Xtensa filetest suite down. The family stays behind
`LP_XT_BUILTINS_F32=1`, and `lpvm-native`'s
`a_builtin_routed_float_op_resolves_and_runs` skips loudly rather than failing
when the resident image lacks it.

**This is a different defect from the constant-pool one it was found behind.**
That one was "the f32 family does not compile for Xtensa"; it is worked around
and the image now builds. This one is "the f32 family does not *fit* alongside
shader code". Fixing the first exposed the second.

It is also a live question for **M7 P5**: `fw-esp32s3` is not bound by this
linker script or by the emulator's 128 KiB model, so the capacity limit is the
*host emulation* path's, not the device's. Worth confirming before assuming P5
inherits it.

## Candidate resolutions (not chosen here — needs a decision)

1. **Widen the emulator's modeled code region.** 128 KiB is the host
   emulator's model, not silicon; the S3 has far more IRAM. Touches
   `lp-xt-emu`'s `BoardProfile` and both linker scripts, and `lp-xt-elf`'s
   `DATA_BASE = CODE_DBUS_BASE + CODE_REGION_LEN/2` convention.
2. **Give the shader its own region** rather than the tail of the builtins
   region, removing the coupling between builtins size and maximum shader size
   entirely. Largest change, best end state.
3. **Split the f32 builtin feature** so only the arithmetic family M7 D4
   actually routes to (divide, sqrt, rounding, min/max, conversions,
   transcendentals) is linked, leaving the generative-noise family — which is
   most of the 47 KB — rv32/host-only. Cheapest, and the noise builtins are not
   what M7's lowering calls. Changes a feature contract M5 owns, so it is
   Yona's call.

(3) is the smallest thing that unblocks M7's host path; (1) or (2) is what
makes the limit stop recurring.
