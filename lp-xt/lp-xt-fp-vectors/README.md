# lp-xt-fp-vectors

The **M6 Xtensa FP conformance corpus**: a deterministic, float-free, `no_std`
generator for the vectors `lp-xt-emu` predicts and the desk ESP32-S3 answers.

This crate holds no expected results. It generates *inputs*. The predictions
live in `lp-xt/lp-xt-emu/tests/fixtures/fp/`, and the silicon answers arrive in
M6 P6.

## Why it is shaped this way

**No dependencies, and the PRNG is written out.** The crate compiles into
`fw-esp32s3`'s conformance harness as well as into host tests, and both sides
must produce byte-identical vectors. A dependency — including `rand` — is a way
for them to drift, and the campaign's whole value rests on "vector 41 337" being
the same thing on both sides.

**Float-free.** The generator emits raw `u32` bit patterns and performs no
floating-point arithmetic anywhere. If it built its inputs with `f32` it would be
constructing them with the very semantics under test; on the device it would need
the FPU it is trying to characterize. This is *checked*, not promised — a test
reads the crate's own source and fails if `f32` or `f64` appears in a code
position.

**Pure and index-addressable.** `vector(family, index)` is a function with no
state, so P6 can re-run one vector alone while bisecting a divergence instead of
replaying a batch to reach it.

**Fingerprinted.** `fingerprint()` hashes every vector of every family. Both
sides print it; the campaign aborts on a mismatch, and
`lp-xt-emu/tests/fp_conformance.rs` fails if the committed corpus was generated
by a different version of this crate. That is what makes "the same code on both
sides" a check rather than an assumption.

## The six families

| ID | `Family` | Vectors | What it must reach |
|---|---|---|---|
| F1 | `Rounding` | 2592 | exact ties and near-ties for add/sub/mul, in both tie-break directions, replayed under **all four** FCR modes so the ADR can say whether FCR is honored at all |
| F2 | `NanPayload` | 658 | every binary op × {qNaN, sNaN} × {A, B, both} × several distinct payloads — so "which operand's payload survives" is *answerable*, not inferable — plus ten hardware-**generated** NaNs (`inf − inf`, `inf × 0`, …) for the default-NaN shape |
| F3 | `Denormal` | 350 | subnormal-in/normal-out, normal-in/subnormal-out, and both-subnormal, kept **separable**: input flush and output flush are different behaviors and a family that conflates them cannot tell you which one this silicon does |
| F4 | `SignedZero` | 204 | ±0 through negate, abs, multiply, add, the compares (`+0 == −0` must be true), and the conversions, in both operand orders |
| F5 | `DivSqrt` | 1296 | the four estimate instructions over a strided significand sweep at four exponents and both parities, plus the manual's divide and square-root **sequences** over powers of two, values near 1, very large and very small, denormals, ±0, ±inf, NaN, and `x/x` |
| F6 | `Convert` | 528 | `float.s`/`ufloat.s` from `i32::MIN`/`MAX`, `u32::MAX`, ±1, and values past 2²⁴ that need rounding; `trunc.s`/`utrunc.s`/`round.s`/`floor.s`/`ceil.s` at ±2³¹, ±inf, NaN, ±0, and just inside/outside range — plus a scale-immediate sweep, because the field exists even though M7 only emits 0 |

5630 vectors total. Sizes are chosen by "cheap to get wrong, expensive to
discover later", not by volume.

### F5 is a sample, and the extraction is not

`recip0.s`, `rsqrt0.s`, `sqrt0.s`, and `div0.s` read an implementation-defined
lookup ROM. Sampling it would let the emulator be *close*, which is exactly the
failure M6 exists to prevent. F5's estimate block here is a representative sweep
for conformance; the **exhaustive table extraction** — sweep the whole
significand for a representative exponent, run-length encode, confirm the
exponent rule separates — is a separate P6 mechanism, and the result is loaded as
a table so those instructions become exact *by construction*.

## Adding a family

1. Add the variant to `Family`, `Family::ALL`, `label()`, and `name()`. The
   discriminants are stable — they appear in corpus files and device output — so
   append, do not renumber.
2. Add its `const` input tables and its `fN(index) -> Vector` function next to
   the others. Integer arithmetic only.
3. Add its arm to `count()` and `vector()`.
4. **Write the coverage assertions with it**, in the same commit. Coverage that
   is asserted is coverage; coverage that is described is a hope.
5. Update `the_fingerprint_is_stable` and regenerate the predictions:
   `UPDATE_FP_GOLDENS=1 cargo test -p lp-xt-emu --test fp_conformance`, then
   **read the diff**.

## What a vector is not

`Vector` carries no expected result, on purpose. A generator that also knew the
answers would be the oracle, and then the corpus could only ever confirm itself.
The oracle is `lp-xt-emu`, its predictions are committed before any hardware
runs (M6 D2), and silicon is the thing they are checked against.
