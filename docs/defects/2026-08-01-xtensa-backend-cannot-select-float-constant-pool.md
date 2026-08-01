---
status: open           # the backend is still broken; our source avoids it (see "The workaround")
found: 2026-08-01      # how: build (M7 P4, flipping the Xtensa builtins image to float-f32)
worked-around: 2026-08-01   # rgb2hsv_f32.rs; f32 image and filetests unblocked
area: lp-xt/lps-builtins-xt-app, lp-shader/lps-builtins, esp Rust toolchain
class: upstream-toolchain-limitation
related:
  - docs/design/float.md
  - docs/defects/2026-08-01-xt-f32-builtins-exhaust-the-emulator-code-region.md
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

**Trigger** — a *constant aggregate* of floats. The backend can materialize
the address of ordinary data but has no selection rule for `PCREL_WRAPPER`
over a `TargetConstantPool` node, so any `[N x float]` constant LLVM chooses to
put in the constant pool is fatal. Only the first such constant to be selected
is reported, which made the trigger easy to misattribute (see below).

**rv32 is unaffected** — `scripts/build-builtins.sh` has passed
`--features float-f32` since M5 and builds the same source cleanly. This is
specific to the Xtensa target's backend.

**It is also load-bearing for the Q32 path.** `scripts/filetests.sh` builds
this image before running the `xtn.*` / `xtlpn.*` targets, so a compile failure
here takes the entire Xtensa filetest suite down with it — not just the f32
half.

## What was actually wrong (2026-08-01)

The first draft of this record named
`lps-builtins/src/builtins/lpfn/generative/snoise/snoise2_f32.rs`'s 8-entry
`[f32; 2]` gradient LUT as the trigger, inferring it from the reported
constant `[0.0, -1.0]` — which happens to be that table's arm 3. **That was
wrong.** The compiler names the containing function, and it is
`__lp_lpfn_rgb2hsv_f32`:

```rust
// lps-builtins/src/builtins/lpfn/color/space/rgb2hsv_f32.rs
let p: [f32; 4] = if g < b {
    [b, g, -1.0, 2.0 / 3.0]
} else {
    [g, b, 0.0, -1.0 / 3.0]
};
```

The two constant lanes of Hocevar's `p` term are selected together, and LLVM
materializes the pair as `[2 x float] [0.0, -1.0]` — the same bytes the
snoise2 arm would have produced, from an entirely different function.

**Exactly one site needed changing.** With `rgb2hsv` fixed the image builds,
and every other suspect this record named — `snoise2`'s and `snoise3`'s
gradient LUTs, `worley2/3`'s offset tables, `gnoise3_tile`'s `CORNERS`,
`psrdnoise2/3` — compiles unchanged and is present in the built ELF. The
pattern that trips the backend is narrower than "a float lookup table": it is a
*fully constant* aggregate that survives to instruction selection. A LUT
indexed by a runtime value becomes an ordinary global; a `match` whose arms are
runtime-scaled (`worley`'s `[diag, diag, 0.0]`) is not a constant aggregate at
all.

## The workaround (shipped)

`rgb2hsv_f32.rs` selects the two constant lanes as **integer bit patterns** and
bitcasts afterwards, so no float constant aggregate is ever formed:

```rust
const P_Z_IF: u32 = (-1.0f32).to_bits();      // const-evaluated from the
const P_W_IF: u32 = (2.0f32 / 3.0).to_bits(); // original literals
...
f32::from_bits(if g_lt_b { P_Z_IF } else { P_Z_ELSE })
```

Chosen over computing the gradient arithmetically because it is *provably*
bit-identical rather than argued: the constants are the literals, put through
`f32::to_bits` at compile time. Sign-of-zero is the specific hazard an
arithmetic rewrite would have introduced (`0.0 * -1.0` is `-0.0`, and the
literal table's zero lane is `+0.0` on both sides).

The site carries a comment naming this file and stating it is an upstream
limitation, not a preference, with a slot for the upstream issue URL. The
pre-workaround form is kept verbatim in the test module as an oracle:
`hocevar_p_is_bit_identical_to_the_literal_form` compares the two lane-by-lane
over a grid that crosses `g == b`, plus `-0.0` and NaN. These builtins are
shared with rv32 and wasm, where the original compiles fine, so drift would be
a silent behaviour change on targets that never had the problem.

`LP_XT_BUILTINS_F32=1 scripts/build-builtins-xt.sh` now succeeds — the thing
this defect blocked — and
`lp-shader/lpvm-native/tests/xt_pipeline_f32.rs`'s
`a_builtin_routed_float_op_resolves_and_runs` is no longer `#[ignore]`d: it
links the shader against the builtins base image and resolves `ffloor` to the
real `__lp_lpir_ffloor_f32` on the M6 emulator.

**The feature was briefly still not the default, for an unrelated reason.**
Making it unconditional was attempted and reverted on measurement: with the
family in, the image's `.text` was 113,757 B against link.ld's 112 KiB, leaving
931 bytes of the code region for shader code, and the xtn.q32 filetest suite
dropped from 849/849 files to 522/849 on link failures. That was a separate
defect —
`docs/defects/2026-08-01-xt-f32-builtins-exhaust-the-emulator-code-region.md`
— and fixing this one is what exposed it. It is now fixed in turn (the image is
flash-resident, so it never shared the code region to begin with), and
`--features float-f32` **is** the Xtensa builtins image's unconditional
default.

## Why this stays open

The backend limitation is untouched. A new `[f32; N]` constant aggregate
anywhere in `lps-builtins` will fail the same way, with the same one-function-
at-a-time reporting, and **M7 P5's `fw-esp32s3` naming `lps-builtins/float-f32`
is exposed to it**. It closes when the esp fork can select
`PCREL_WRAPPER(TargetConstantPool)` and the toolchain is pinned past that.

## Remaining resolutions

1. **Report upstream** to `esp-rs/rust` and pin the toolchain once fixed. Not
   yet filed; the comment in `rgb2hsv_f32.rs` and the one in
   `build-builtins-xt.sh` are shaped for the URL to be dropped in.
2. **Split the f32 builtin feature** so the generative-noise family is
   separately gated. No longer needed — this was the fallback for "Xtensa
   cannot have the noise family at all", and it can.
