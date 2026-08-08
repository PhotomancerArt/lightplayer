---
status: open
found: 2026-07-30      # how: hardware-walk
area: lpvm-native / lpvm-wasm (Q32 execution)
class: backend-contract-divergence
related:
  - 2026-07-27-wasm-q32-fabs-stack-leak.md
  - 2026-07-27-cranelift-q32-floor-ceil.md
---
# Q32 renders differ in the last bit between the native code generator and wasmtime

**Symptom** — Rendering one project through three engines and diffing the
ws281x bytes byte-for-byte:

| Engine | frame checksum |
|---|---|
| ESP32-S3, `lpvm-native` `isa/xt` JIT, on silicon | `0xafef2def` |
| host, `lpvm-native` `rt_emu` (rv32) | `0xafef2def` |
| host, `lpvm-wasm` `rt_wasmtime` | `0x4e6015de` |

One byte of 192 differs, by exactly 1: LED 53, green channel, 174 on the two
native engines against 175 on wasmtime. Switching `glsl_opts.div` from
`reciprocal` to `saturating` moves the disagreement rather than removing it
(two bytes instead of one), so this is not the documented ~0.01% error of the
reciprocal approximation.

The same project **without** the divergence trigger is bit-identical across all
three — 192 of 192 bytes, `0x55772254` — which is what is checked in by
`examples/shader-oracle`. The trigger found so far is `smoothstep(0.05, 0.95,
x)`: with unit-width edges the normalisation folds away and everything agrees;
with real edges it does not.

**Root cause** — Not localised. What the three-way comparison *does* establish
is the shape: the ESP32-S3's Xtensa JIT agrees with rv32 native codegen
**exactly**, and both differ from wasmtime. So this is a difference between the
native code generator and the WASM one, present on the ESP32-C6 for as long as
both have existed, and **not** an Xtensa defect — which is what the walk that
found it was actually looking for.

Magnitude is one part in ~44,700 of a Q32 sample, invisible until the u16 → u8
output reduction rounds it across a boundary and turns it into one LSB of one
channel.

**Fix** — None yet. Filed on discovery, per the registry's found-not-yet-fixed
rule. **Still open, and unchanged by the amendment below** — the Q32 last bit
this entry is about has not been located or fixed, and `rt_emu` remains the host
oracle for firmware work.

> **Amendment, 2026-08-07 — one symptom filed here was never this defect.**
> `2026-08-02-f32-shader-cannot-render-a-frame.md` recorded that removing the
> `rt_wasmtime` frame guards made an **f32** shader render "uniformly one count
> low against the rv32-emulator oracle", and attributed it to this entry; the
> guards' own comment, and the float-native-mode plan's Q7, carried that
> attribution forward and asked for the one count to be **classified** under
> `docs/design/float.md` (Guaranteed → fix; Unspecified → drop the guard).
>
> Measured on 2026-08-07: the f32 one-count has a **separate and unrelated
> cause** — `lpvm-wasm`'s inline `FloatMode::F32` unorm lowering used the GPU
> `v * 65535` scale where the rest of the tier uses the documented
> `floor(v * 65536)` clamped convention. It is a Guaranteed-class violation,
> now fixed, with the wasm f32 frame path bit-identical to the oracle:
> `2026-08-07-wasm-f32-unorm-scale-convention.md`. The frame guards were lifted
> on that, not on a classification.
>
> The tell was in the data all along and is worth keeping: **this** defect is a
> *sparse* divergence (one byte of 192, only when `smoothstep` has real edges),
> and what was filed under it was *uniform* (every channel, every sample, always
> exactly one). A rounding divergence and a scale error do not have the same
> distribution. Nothing about the Q32 finding above is revised.

**Regression coverage** — `lp-app/lpa-server/tests/shader_oracle_frame.rs`
renders the committed project through both host engines every run and prints
`[ORACLE-DIFF]`, so a *future* divergence on the checked-in project shows up in
the transcript. It deliberately does not assert equality: the number is
evidence a human should read, and failing the test would hide it. There is no
coverage of the divergent variant, because pinning a value would pin whichever
engine happened to write it — the right home is a filetest, see the lesson.

**Lesson** — Q32 is specified to be bit-exact across targets, and the filetest
suite is the instrument that enforces it. This escaped because **wasmtime is
not a filetest target the way rv32 native is**: 31,587 green cases say the
native backends agree with the interpreter, and say nothing about the engine
the studio and every host test actually run. A whole-frame comparison across
engines found in one afternoon what the per-expression suite structurally
cannot.

Two things follow. The narrower one: the *right* host oracle for firmware work
is `rt_emu`, not wasmtime — same code generator, one ISA over — and any harness
that quotes "the host" should say which. The broader one: the Xtensa backend
still has no bit-exactness target of its own (`xtn.q32` needs a cross-compiled
builtins image and remains future work). It is only because rv32 emulation was
available as a third point that this was diagnosable at all; had the split been
Xtensa-against-both, this entry would say "device disagrees, cause unknown".
That is the argument for finishing `xtn.q32`, stated in evidence rather than in
principle.
