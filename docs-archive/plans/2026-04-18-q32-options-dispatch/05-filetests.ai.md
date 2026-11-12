# Phase 5 — Filetests

## Scope of phase

Add 4 new filetests under `lp-shader/lps-filetests/filetests/scalar/float/`
that exercise the new fast-math `Q32Options` modes end-to-end through
GLSL → LPIR → backend → execution. Each uses the existing
`// compile-opt(key, value)` directive to opt the test into the relevant
mode (default is Saturating across the board).

**Out of scope:**

- Changes to the filetest runner.
- New wasm-specific filetests (existing runner must already exercise wasm
  if the wasm backend is configured for filetests; if not, document for
  phase 6 follow-up).
- Cross-mode property tests (covered by phase 3 + phase 4 unit tests).

## Code Organization Reminders

- One mode per file; named with the `q32fast-` prefix as decided in
  planning notes.
- Place new files alphabetically among siblings.

## Sub-agent Reminders

- Do **not** commit. The plan commits at the end.
- Do **not** expand scope.
- Read several existing files in `scalar/float/` first to match the
  conventions exactly (header comments, expected-output format,
  `compile-opt` syntax, fixture IO).
- If the runner does not invoke wasm by default, do **not** force it —
  the per-backend unit tests in phases 3 and 4 are the bit-identity
  guarantee. Filetests are end-to-end smoke + integration.
- If something blocks completion, stop and report back.

## Implementation Details

### File 1: `q32fast-add-sub.glsl`

Path: `lp-shader/lps-filetests/filetests/scalar/float/q32fast-add-sub.glsl`

Test that wrapping add/sub produces correct **modular** results, including
overflow-wrap (which the saturating mode would clamp).

Approach: take two inputs near `MAX_FIXED`, add them in the shader, return
the wrapping result. Saturating mode would return `MAX_FIXED`; wrapping
returns the actual modular value. The test uses the `q32.add_sub, wrapping`
opt and expects the modular value.

Sketch (adapt to the actual filetest GLSL fixture conventions in this
repo — read existing files first):

```glsl
// compile-opt(q32.add_sub, wrapping)
//
// Verify that q32.add_sub=wrapping uses i32.add semantics:
// MAX_FIXED + 1 wraps to MIN_FIXED instead of saturating.

uniform float a;  // expect feed: ~32767.999984741 (~ MAX_FIXED in fixed point)
uniform float b;  // expect feed: 1.0

void main() {
    float sum = a + b;       // wrapping in q32 fast mode
    float diff = b - a;      // wrapping subtract
    // ... emit sum and diff into the test fixture's output channel ...
}
```

Add a companion expected-output annotation per the runner's convention
(`// expected: ...` or similar — read existing files).

### File 2: `q32fast-mul.glsl`

Path: `lp-shader/lps-filetests/filetests/scalar/float/q32fast-mul.glsl`

Test wrapping multiply (`((a*b) >> 16) as i32`, modular). Inputs that
would saturate under Saturating mode but produce a deterministic modular
result under Wrapping.

```glsl
// compile-opt(q32.mul, wrapping)
//
// Verify that q32.mul=wrapping does not saturate.

uniform float a;
uniform float b;

void main() {
    float p = a * b;
    // ... emit p ...
}
```

Pick `a, b` such that the Saturating helper would clamp and the Wrapping
expansion (5-VInst on native, 6-op on wasm) produces a known modular
value. For example, `a = b = 200.0` (in fixed: 200<<16 = 13_107_200);
their product = 40_000.0 in real-math, fits in MAX_FIXED (~32767), but
multiplying two large enough Q32 values where bits 47:32 are nonzero will
demonstrate the wrapping. Choose carefully.

A safer choice: pick `a` very large (~30_000) and `b` very small fractional
(~0.0001) — the product fits cleanly, no wrap, but exercises the
multiplier and proves the new code path runs (default mode would also
match here, so this is mostly a "doesn't crash" test). To **prove
wrapping**, pick something like `a = b = 0x7FFF_0000` (interpreted as
fixed: ~32767), product real = ~1.07B, way above MAX_FIXED — Saturating
clamps to MAX_FIXED, Wrapping returns the actual modular bits. Compute
expected modular value:

```text
a = b = 0x7FFF_0000 = 2147418112 (decimal)
(a * b) = 2147418112 * 2147418112 = 4_611_545_280_675_807_232_... 
truncated mod 2^64 = bits we care about for >> 16:
((a as i64 * b as i64) >> 16) as i32
```

Compute with a quick Rust REPL or test code; embed the expected i32 as a
constant in the test annotation.

### File 3: `q32fast-div-recip.glsl`

Path: `lp-shader/lps-filetests/filetests/scalar/float/q32fast-div-recip.glsl`

Verify reciprocal division produces results within tolerance of the
saturating helper for "normal" cases.

```glsl
// compile-opt(q32.div, reciprocal)
//
// Verify that q32.div=reciprocal produces approximately correct results
// for normal-magnitude inputs (within ~0.1% of true value).

uniform float a;  // e.g. 10.0
uniform float b;  // e.g. 3.0

void main() {
    float q = a / b;   // expected ~3.333..., reciprocal ~equal within tolerance
    // ... emit q ...
}
```

Expected value: `a / b` ± reciprocal tolerance. The runner needs to
support a tolerance comparison — if it doesn't, embed the expected exact
i32 value (computed via `__lp_lpir_fdiv_recip_q32` algorithm) and assert
exact equality. Read existing filetests to see whether tolerance is
supported; a search for `tol` or `epsilon` in `scalar/float/*.glsl` will
tell you.

### File 4: `q32fast-div-recip-by-zero.glsl`

Path: `lp-shader/lps-filetests/filetests/scalar/float/q32fast-div-recip-by-zero.glsl`

Verify the `divisor == 0` saturation guard works in fast mode (must not
trap; must return MAX/MIN/0 matching saturating helper policy).

```glsl
// compile-opt(q32.div, reciprocal)
//
// Verify that q32.div=reciprocal does not trap on divisor==0; saturates
// to MAX_FIXED for positive dividends, MIN_FIXED for negative, 0 for 0/0,
// matching __lp_lpir_fdiv_q32 saturation policy.

uniform float a;     // e.g. 1.0
uniform float zero;  // e.g. 0.0

void main() {
    float q1 = a / zero;          // expect MAX_FIXED (positive / 0)
    float q2 = (-a) / zero;       // expect MIN_FIXED (negative / 0)
    float q3 = zero / zero;       // expect 0       (0 / 0)
    // ... emit q1, q2, q3 ...
}
```

Expected values: the Q16.16 representations of MAX_FIXED, MIN_FIXED, 0.
Read existing filetests to see how these constants are typically expressed
in expected output.

### Validate the new filetests

From workspace root, run the filetest suite. The exact command depends on
the runner — likely one of:

```bash
cargo test -p lps-filetests
# or
cargo test -p lps-filetests-runner
# or
turbo test --filter=lps-filetests
```

Find the right command by reading `lps-filetests`'s `Cargo.toml` or its
README.

All 4 new files must pass. All existing filetests must still pass
(defaults unchanged).

If the runner exercises only one backend by default and you need both
backends covered: do **not** modify the runner in this phase. Note in
your phase report that wasm backend coverage in filetests needs follow-up
in phase 6 if the runner is native-only.

## Definition of done

- 4 new files under `lp-shader/lps-filetests/filetests/scalar/float/`:
  - `q32fast-add-sub.glsl`
  - `q32fast-mul.glsl`
  - `q32fast-div-recip.glsl`
  - `q32fast-div-recip-by-zero.glsl`
- Each file uses the appropriate `// compile-opt(...)` directive.
- Each file has a clear header comment explaining intent.
- All 4 new filetests pass.
- All existing filetests still pass.
- No new warnings.
