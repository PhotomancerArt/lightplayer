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
rule.

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
