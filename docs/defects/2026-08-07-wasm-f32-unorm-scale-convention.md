---
status: fixed
found: 2026-08-07      # how: live-debugging
fixed: this change
area: lpvm-wasm/src/emit/ops.rs (F32 unorm lowering)
class: split-source-of-truth
related:
  - 2026-08-02-f32-shader-cannot-render-a-frame.md
  - 2026-07-30-q32-native-vs-wasmtime-last-bit.md
  - ../design/float.md
  - ../adr/2026-08-08-float-semantics-per-target-representation.md
---
# The wasm f32 unorm lowering used the GPU scale convention, so every frame channel read one count low

**Symptom** — with the `FloatMode::Q32` guards removed from `rt_wasmtime`'s
`call_render_texture` / `call_render_samples`, a Float shader renders
structurally correct frames whose every non-zero, non-saturating channel is
**exactly one count below** the rv32 oracle. Driving
`lps-filetests/tests/f32_render_entry.rs`' shared table through
`WasmLpvmEngine` instead of `NativeEmuEngine`:

```
samples q32 = [0, 0, 16384, 65535, 32768, 65535, 16384, 65535, 16384, 49152, ...]
samples f32 = [0, 0, 16383, 65535, 32767, 65535, 16383, 65535, 16383, 49151, ...]
```

This was recorded on 2026-08-02 and **attributed to the wasmtime last-bit
divergence** (`2026-07-30-q32-native-vs-wasmtime-last-bit.md`) — the guards'
comment, the f32-frame defect's amendment, and the float-native-mode plan all
carried that attribution forward. **The attribution was wrong**, and the
uniformity is what gives it away: a last-bit rounding divergence moves one
sample in a frame, not every channel of every sample by the same amount.

**Root cause** — `lpvm-wasm`'s inline `FloatMode::F32` lowering of the four
unorm ops used the **GPU convention** (`v * 65535`, `code / 65535`,
`v * 255`, `code / 255`) where every other tier uses the **Q32-inherited
convention** (`floor(v * 65536)` clamped to `[0, 65535]`, `code / 65536`) that
`docs/design/float.md` §7 fixes for the frame boundary and
`lps_builtins::builtins::lpir::unorm_conv_f32` implements. Both scales are
defensible in isolation; only one is this product's. Every observed code is
`floor(v * 65535)` exactly — `0.25 → 16383`, `0.5 → 32767`, `0.875 → 57343`.

The conversion therefore had **two producers and no hand-off check**: the
`lps-builtins` function (which the wasmtime host trampoline calls, and which
carries its own passing unit tests asserting the 65536 convention) and the
inline emitter in the same crate (which the compiled module actually executes).
Each was internally consistent; nothing compared them.

Two things masked it. The wasm f32 inline unorm path is **self-consistent** —
`FtoUnorm16` and `Unorm16toF` used the *same* wrong scale, so anything that
round-trips inside one module agrees with itself. And the corpus does not reach
these ops at all: `LpirOp::FtoUnorm16` is emitted only by the synthesised frame
wrappers (`lp-shader/src/synth/`), which no filetest drives, `pack-unorm.glsl`
is `@unsupported(*)` on every target, and texture sampling calls the *builtin*
rather than the op. Confirmed: `scripts/filetests.sh --target wasm.f32` reads
6353/6353, 852/852 files, identical before and after the fix.

**Fix** — the four `FloatMode::F32` arms in `lpvm-wasm/src/emit/ops.rs` now
emit the documented convention: `FtoUnorm16` clamps to `[0, 1]`, scales by
`65536`, truncates, and clamps the code to `65535` (mirroring the Q32 arm's
select-based clamp); `FtoUnorm8` the same with `256`/`255`; `Unorm16toF` and
`Unorm8toF` divide by `65536` / `256`. Q32 arms are untouched.

With that, the wasm f32 frame path is **bit-identical to the rv32 oracle** on
the shared table — which is what `float.md` §3 and §7 require of it, both
conversions being Guaranteed-class. There is nothing target-defined here to
document.

**Regression coverage** — `lp-shader/lps-filetests/tests/f32_render_entry_wasm.rs`,
the wasm sibling of the rv32 `f32_render_entry.rs`, entering through the
product's door (`compile_px_desc(...).with_float_mode(Float)` →
`sample_points_rgba16` / `render_frame`) and asserting the **same table
exactly**, in both modes. It fails with the pre-fix constants (that is how the
codes above were captured) and its Q32 rows are the byte-identical control.
`FtoUnorm8` / `Unorm8toF` remain uncovered because no 8-bit
`TextureStorageFormat` exists yet — their correction is consistency, not a
tested claim.

**Lesson** — *an inline lowering that mirrors a library function is a second
implementation, and the fact that both have green tests is exactly the
condition under which they drift.* The pattern is the registry's
`split-source-of-truth`, and here it was compounded by a
misdiagnosis that survived four months of restatement: because a
plausible known defect (`q32-native-vs-wasmtime-last-bit`) already lived at
this seam and predicted "wasmtime disagrees in the last bit", the observation
"wasmtime is one count off" was filed under it without anyone checking whether
the *shape* matched. It did not — a rounding divergence is sparse and a scale
error is uniform, and the distinction was visible in the very first
measurement. When a new symptom is claimed by an existing defect, the cheap
guard is to ask what the existing defect *predicts about the distribution*, not
just about the magnitude.
