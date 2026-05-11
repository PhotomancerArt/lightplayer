# Summary — lpvm-native Q32 op inlining

## What changed

- `lpvm-native::lower::lower_lpir_op` switched from `Result<VInst, _>` to
  `Result<(), _>` with sink-param `out: &mut Vec<VInst>` and a
  `TempVRegs` watermark for fresh intermediate vregs (added to
  `vinst.rs`). Lifts the 1-LPIR-op → 1-VInst constraint and unblocks
  multi-VInst expansion. `sym_call` adopted the same sink shape.
- Inlined 7 Q32 ops that previously went through `sym_call` runtime
  helpers:
  - `Fabs` — branchless 3-VInst sequence (`SraiS(31) + Xor + Sub`)
    matching `wrapping_neg` exactly (incl. `i32::MIN.abs() == i32::MIN`).
  - `Fmin` / `Fmax` — `Icmp + Select`.
  - `Unorm16toF` — `IConst32(0xFFFF) + And`.
  - `Unorm8toF` — `Andi(0xFF) + Slli(8)`.
  - `FtoUnorm16` — clamp to `[0, 65535]` via two compare-and-select
    stages (6 VInsts, 5 temps).
  - `FtoUnorm8` — `SraiS(8)` + clamp to `[0, 255]` (7 VInsts, 6 temps).
  - (`Fneg` was already inline as `VInst::Neg` — no work.)
- Q32 helpers in
  `lps-builtins/src/builtins/lpir/{float_misc,unorm_conv}_q32.rs` are
  kept as the reference implementation; documented as such.
- `lower.rs` module header documents the four-tier inline-vs-call
  policy and per-op rationale.
- 7 new `lower.rs` unit tests assert the new VInst sequences (one per
  inlined op except `Fmin`/`Fmax`/`Fabs` which got dedicated tests; the
  remaining 22 unit tests for non-inlined ops kept their `len == 1`
  guard with refreshed assertion message).

## What didn't change

- LPIR surface (no new ops, no new `LpirOp` variants).
- `VInst` surface (existing variants suffice).
- Cranelift / WASM / emu backends.
- Op semantics — all expansions match helper bit-for-bit on i32.
- `Fadd`/`Fsub` lowering — still routed through saturating helpers
  (see follow-up).

## Validation

- `cargo test -p lpvm-native --lib` — 202 passed (was 195 + 7 new).
- `cargo test -p lps-filetests` — full filetest suite passes,
  including `rv32n_smoke`, `lpfn`, `debug/rainbow`, and the
  `__render_texture` synth tests that exercise `FtoUnorm{16,8}` /
  `Unorm{16,8}toF` on every channel of every pixel.
- `cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu`
  — both pass (canonical AGENTS.md validation).
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf` —
  pass (release-esp32 profile, esp32c6+server features).
- `cargo check -p fw-emu --target riscv32imac-unknown-none-elf` —
  pass (release-emu profile).
- `cargo check -p lp-server` / `cargo test -p lp-server --no-run` —
  pass.

## Side effect: pre-existing M2.0 fixups landed in the same commit

- `RenderTextureEntry` and the `NativeJitInstance::render_texture_cache`
  field gained `pub(crate)` (they were `private`, blocking sibling
  module construction; introduced as `private` in M2.0 commit
  `0dcb4015` and only manifested as a `fw-emu` cross-compile failure
  not exercised by `cargo test -p lpvm-native --lib`).
- `target/.../lps-builtins-emu-app` ELF regenerated via
  `scripts/build-builtins.sh` — the embedded builtins binary needed to
  pick up the four `__lp_lpir_*_unorm*_q32` symbols added in commit
  `0ed986b3`. Not committed (it's a build artifact under `target/`).

## Follow-ups

- **Q32Options dispatch (next priority).** Wire
  `Q32Options::add_sub` (already defined in
  `lps-q32/src/q32_options.rs` since the pre-LPIR Cranelift era but
  consumed by no codegen path today) through the lowering pipeline so
  `Fadd`/`Fsub` can pick between inline `add`/`sub` (Wrapping mode)
  and the saturating helper (Saturating default). Pair with an
  inlining of the wrapping path. Architectural: this is the sole
  reason `Fadd`/`Fsub` weren't inlined here.
- **Inline `Fmul`** — saturating mul is `mul + mulh + reassemble +
  saturate` (~6-8 RV32 insts); wrapping mul is ~3-4. Pair with the
  Q32Options work above (a `Q32Options::mul` already exists).
- **Inline `ItofS`/`ItofU`** clamp+shift expansions — sequence is
  ~5 VInsts, roughly call cost; deprioritized.
- **Inline `FtoiSat[SU]`, `Ffloor`/`Fceil`/`Ftrunc`/`Fnearest`** —
  saturation / rounding-mode review.
- **Zbb-bearing `IsaTarget`** — collapse `Fmin`/`Fmax` to single
  insts when a target supports it (ESP32-C6 does not).
