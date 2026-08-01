---
status: open
found: 2026-08-01      # how: build (M7 P4, flipping the Xtensa builtins image to float-f32)
area: lp-xt/lps-builtins-xt-app, lp-shader/lps-builtins, esp Rust toolchain
class: upstream-toolchain-limitation
related:
  - docs/design/float.md
---
# The esp Xtensa backend cannot select a float constant pool, so `lps-builtins/float-f32` does not compile for Xtensa

**Symptom** — Building the Xtensa builtins image with the f32 family enabled
fails in the compiler backend, not in our code:

```
rustc-LLVM ERROR: Cannot select: 0x10b3d4770: i32 = XtensaISD::PCREL_WRAPPER
  TargetConstantPool:i32<@.LCP162_3 = internal constant [2 x float]
  [float 0.000000e+00, float -1.000000e+00]> 0
error: could not compile `lps-builtins-xt-app` (bin "lps-builtins-xt-app")
```

Reproduced with `scripts/build-builtins-xt.sh` after adding
`--features float-f32`, on
`rustc 1.95.0-nightly (95e5bda86 2026-04-15) (1.95.0.0)` from
`~/.rustup/toolchains/esp`, target `xtensa-esp32s3-none-elf`.

**Not an optimization artifact.** The `lp-xt/fixtures` release profile uses
`opt-level = "s"` with `lto = "fat"`. Rebuilding with
`CARGO_PROFILE_RELEASE_OPT_LEVEL=1 CARGO_PROFILE_RELEASE_LTO=false` fails the
same way, only earlier — in `lps-builtins` (lib) itself rather than in the
final binary. So the failure is the backend's, at any optimization level, and
is not confined to the image crate.

**Trigger** — `lps-builtins/src/builtins/lpfn/generative/snoise/snoise2_f32.rs`
has an 8-entry gradient lookup table written as a `match` returning
`[f32; 2]`. LLVM promotes the arms to constant-pool entries, and the Xtensa
backend has no selection rule for `PCREL_WRAPPER` over a `TargetConstantPool`
node — it can materialize the *address* of ordinary data, but not of a
constant-pool entry. The reported constant, `[0.0, -1.0]`, is that table's
arm 3. Other `[f32; N]` tables in the generative-noise family
(`worley2/3`, `gnoise3_tile`, `psrdnoise2/3`, `snoise3`, `rgb2hsv`) are
plausible further instances; only the first to be selected is reported.

**rv32 is unaffected** — `scripts/build-builtins.sh` has passed
`--features float-f32` since M5 and builds the same source cleanly. This is
specific to the Xtensa target's backend.

**It is also load-bearing for the Q32 path.** `scripts/filetests.sh` builds
this image before running the `xtn.*` / `xtlpn.*` targets, so a script that
passes `--features float-f32` unconditionally takes the entire Xtensa filetest
suite down with it. The feature is therefore **opt-in** — the crate wiring is
in place and `LP_XT_BUILTINS_F32=1` requests it — rather than a default that
would trade a blocked f32 path for a broken fixed-point one.

## Consequence

The Xtensa builtins image **cannot currently carry M5's f32 symbols**. M7's
hardware-float lowering inlines the single-instruction family but routes
divide, sqrt, the rounding family, min/max, the float→int conversions and
every transcendental to those symbols (M7 D4), so on the host emulation path
those calls have nothing to resolve against.

What this does *not* block: everything M7 P3 emits inline — `fadd`/`fsub`/
`fmul`, the sign-bit ops, all six compares, float select, `itof`, float
load/store and the `wfr`/`rfr` boundary transfers. Those are the majority of
the emitted subset and are covered end to end by
`lp-shader/lpvm-native/tests/xt_pipeline_f32.rs`, whose one builtin-calling
case is marked `#[ignore]` pointing here.

It is also a live risk for **M7 P5**: `fw-esp32s3` naming
`lps-builtins/float-f32` will hit the same wall at firmware build time.

## Candidate resolutions (not chosen here — out of M7 P4's scope)

1. **Rewrite the affected LUTs** so LLVM does not form a constant pool — e.g.
   return a tuple, index a `const` array through a `static`, or compute the
   gradient arithmetically. Cheapest, but it is a workaround living in shared
   builtin source with no local explanation, and it must be rediscovered for
   every new `[f32; N]` table.
2. **Report upstream** to `esp-rs/rust` and pin the toolchain once fixed.
3. **Split the f32 builtin feature** so the generative-noise family is
   separately gated, letting Xtensa link the arithmetic builtins M7 D4
   actually needs while the noise family stays rv32/host-only.

(3) is the closest fit to what M7 needs — the routed-to-builtin list in D4 is
arithmetic, not noise — but it changes a feature contract M5 owns, so it is a
decision for Yona rather than for this phase.
