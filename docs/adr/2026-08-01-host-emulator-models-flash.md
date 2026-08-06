# ADR: The host Xtensa emulator models flash, and firmware links into it

- **Status:** Accepted
- **Date:** 2026-08-01
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

`lp-xt-emu` modeled one executable region per board — SRAM1, 128 KiB on the S3
and 92 KiB on classic — and everything went in it: the linked `lps-builtins-xt-app`
image *and* the shader code `rt_emu` JIT-compiles. `lps-builtins-xt-app/link.ld`
split that region 112 KiB text / 16 KiB data, and `rt_emu::xt_image` placed
compiled shader functions in whatever IRAM the image's `.text` left over.

On silicon neither of those things is true. Every ESP32 executes its application
`.text` **from flash through the cache** (XIP; the boot log's `vaddr=0x4200_0020
map` lines are the cache MMU being programmed). Internal SRAM holds the stack,
`.data`/`.bss`, the heap, and — for this product — code a JIT produced at
runtime. Firmware and JIT output do not share an address region; they are not
even in the same address quadrant.

Three consequences arrived together on 2026-08-01, when `--features float-f32`
was tried as the Xtensa builtins image's default
(`docs/defects/2026-08-01-xt-f32-builtins-exhaust-the-emulator-code-region.md`):

1. The f32 family took `.text` from 66,300 B to 113,757 B, leaving **931 bytes**
   of the region for shader code. The `xtn.q32` filetest suite fell from 849/849
   files to 522/849 on link failures. The image and the shader were competing
   for bytes that on hardware they never share.
2. It produced a wrong product conclusion — read as "f32 doesn't fit on classic
   ESP32" — by conflating the shader-JIT budget with the builtins' footprint.
   Classic's 92 KiB bounds JIT'd shader code and says nothing about whether the
   f32 builtins fit, because those are flash-resident there too.
3. It hid a live bug class. A shader→builtin call spans SRAM→flash, ~29 MB on
   the S3, far outside `CALL8`'s ±512 KiB displacement — which is why the
   emitter reaches every call target through a literal-pool slot and `CALLX8`.
   In a single small region an accidentally-direct call would have been *in
   range on the host* and out of range on silicon: passing tests, failing
   hardware.

The defect entry proposed three resolutions: widen the modeled region, give the
shader its own region, or split the f32 builtin feature so less of it links.
**All three preserve the wrong map**, and the first and third also spend real
budget to do so.

## Decision

Model flash. `BoardProfile` gains, per chip:

| | ESP32-S3 | classic ESP32 |
| --- | --- | --- |
| IROM — flash instruction window, read-only, `AliasRule::Identity` | `0x4200_0000` | `0x400D_0000` |
| DROM — flash data window, read-only, not fetchable | `0x3C00_0000` | `0x3F40_0000` |
| image `.data`/`.bss` — plain internal SRAM | `0x3FCA_8000` | `0x3FFD_0000` |

`lps-builtins-xt-app` links as flash-resident firmware into those windows.
`rt_emu::xt_image` places each `PT_LOAD` segment by classifying its `p_vaddr`
against the profile, and compiled shader code gets the **whole** SRAM code
region. Guest stores into either flash window fault; the loader paths write them
anyway, because putting an image into flash is what a flasher does.

Three properties are deliberate:

1. **The SRAM code region did not grow.** 128 KiB on the S3, 92 KiB on classic,
   unchanged. The fix is not more room; it is that the builtins left. Widening
   it would have been the same mistake with a bigger number, and it would have
   made the modeled region stop corresponding to anything.
2. **Bases are hardware; lengths are the model's.** The S3's real `irom_seg` is
   32 MB and the emulator allocates its regions once per host call, so the
   modeled windows are 256 KiB / 64 KiB / 32 KiB — sized to the image with
   headroom. A segment past the end is a loud load error, never a silent wrap.
3. **Provenance is graded and stated.** `board.rs`'s SRAM numbers are
   hardware-measured (FINDINGS E2, the classic C1–C5 ladder). The flash numbers
   are *documented and observed*: esp-hal 1.1.1's MIT-or-Apache-2.0
   `ld/esp32{,s3}/memory.x`, corroborated by `lpc-shared`'s backtrace validator
   and by S3 boot logs. They are labelled as the weaker grade rather than blended
   in, because the difference matters if one turns out wrong. No GPL source
   (binutils, GCC, QEMU) was consulted — see
   `docs/adr/2026-07-29-license-provenance-discipline.md`.

`--features float-f32` becomes the Xtensa builtins image's unconditional
default; `LP_XT_BUILTINS_F32` is deleted.

## Consequences

**Good.** `xtn.q32` is 849/849 files (6336/6336 tests) *with* f32, and
`xtlpn.q32` 849/849 (6385/6385). The largest compilable shader no longer depends
on the size of the builtins — the error message that used to name the builtins'
footprint now names the shader's own size. The classic-ESP32 f32 question is
correctly reframed as a flash-budget and unprobed-LX6-FPU question.

> **Both halves answered 2026-08-06.** Flash: `float-f32` costs the classic
> **+63,472 B** against 1,191,920 B of remaining headroom, so the budget was
> never the constraint it was framed as. FPU: probed, and the LX6 agrees with
> the LX7 on all 5 630 conformance vectors and on the estimate ROMs byte for
> byte — see the §10 amendment in
> `2026-07-31-xtensa-fp-behavior-contract.md`. `float-f32` is on by default in
> `fw-esp32v3` as of that date. The residual cost turned out to be neither
> flash nor numerics but **speed**: ~17 % slower than Q32 at 1500 LEDs.

And the
SRAM→flash call reach is now testable, which `lp-xt/lp-xt-emu/tests/call_range.rs`
does.

That test also found something worth having: on **classic**, IROM sits only
~192 KiB above the SRAM1 I-bus window, comfortably inside `CALL8`'s range. A
direct call from JIT'd code to a builtin would work there. The emitter must stay
indirect because the S3 requires it, **not** because every Xtensa target does —
pinned so it is not "optimized away" after testing on one board.

**Costs.** Every emulator now installs five regions instead of two (~352 KiB
more zeroed allocation per host call, `alloc_zeroed`-backed) and the per-call
load copies the image's ~130 KiB of flash instead of the region's 128 KiB — a
wash in bytes. `Memory` now asserts every region is mutually disjoint in both
address views, not just the shared window; that assertion immediately earned its
keep by catching `SHARED_DBUS_BASE = 0x3F40_0000`, which **is** classic's DROM
base. It moved to `0x3000_0000`, below the lowest address either chip's data bus
decodes (`docs/adr/2026-07-30-xtensa-host-shared-memory.md`, amended).

**Unchanged.** `lp-xt/fixtures/link.ld` still links its guests into the SRAM code
region: those are raw payloads a runner writes into RAM, which is a different
thing from a flash-resident image. Every raw-payload suite — `fp_conformance`,
`conformance`, the metrics and trap tests — is untouched, and `lp-xt-elf`'s
`DATA_BASE = CODE_DBUS_BASE + CODE_REGION_LEN/2` convention is untouched with it.
rv32 is untouched: `GuestImage` gained an empty `regions` list there.

## Alternatives rejected

- **Widen the modeled SRAM code region.** Cheapest, and it makes the number stop
  corresponding to hardware. The 128 KiB figure is the emulator's claim about
  what a device has available for JIT output; inflating it to fit a resident
  image would quietly retire that claim.
- **Give the shader a second SRAM region, builtins keeping the first.** Removes
  the coupling without fixing the map. The call-range bug class would still be
  invisible, because both regions would still be a short hop apart.
- **Split the `float-f32` feature** so only the arithmetic family M7 D4 routes
  to is linked, leaving the generative-noise builtins (most of the 47 KB)
  rv32-only. This spends a real feature contract to buy back space that the
  device never needed spent, and it would have to be re-litigated the next time
  the image grows.
- **Model flash at its true length (32 MB windows).** Correct and unaffordable:
  regions are `Vec`-backed and allocated per host call. Truncating the length
  while keeping the base is the honest compromise, and it is documented as such
  on `MODELED_IROM_LEN`.
