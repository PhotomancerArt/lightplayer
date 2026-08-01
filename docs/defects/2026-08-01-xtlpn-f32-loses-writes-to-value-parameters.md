---
status: open
found: 2026-08-01      # how: M8 registering xtn.f32 / xtlpn.f32 and running the corpus
area: lp-shader/lpvm-native (Xtensa f32 lowering), lps-glsl frontend
class: config-masked-defect
related:
  - docs/adr/2026-08-01-float-mode-as-a-compiler-parameter.md
  - docs/defects/2026-07-30-xtensa-call-argument-clobber.md
---
# `xtlpn.f32` loses writes to value parameters — Lp frontend **and** Xtensa **and** f32, all three required

**Symptom** — a function that modifies its own `in` parameter and returns it
returns the *unmodified argument*:

```glsl
vec3 process_vector_in(vec3 v) {
    v.x += 1.0; v.y += 1.0; v.z -= 0.5;   // (abridged)
    return v;
}
// run: test_param_in_modify_components() ~= vec3(2.0, 3.0, 2.5)
//   expected: vec3(2.0, 3.0, 2.5)
//     actual: vec3(1.0, 2.0, 3.0)      ← the input, untouched
```

Not a crash and not a compile error. A **wrong value**, which is the failure
mode this corpus exists to catch.

## It takes all three axes to reproduce

This is the defining property, and the reason it survived M7:

| Target | Frontend | ISA | Float mode | Result |
|---|---|---|---|---|
| `xtn.f32` | Naga | Xtensa | F32 | **passes** |
| `rv32lpn.f32` | Lp | rv32 | F32 | **passes** |
| `xtlpn.q32` | Lp | Xtensa | Q32 | **passes** |
| **`xtlpn.f32`** | **Lp** | **Xtensa** | **F32** | **FAILS** |

Change any single axis and the bug disappears. M7 validated Xtensa f32 through
`xtn.f32`-equivalent paths (hand-built LPIR, the Naga-frontend corpus, and the
silicon corpus) and every one of them passes. Nothing M7 ran could have found
this, because no M7 test combined the Lp frontend with Xtensa f32 — that
combination did not exist as a target until M8 registered it.

That is the whole argument for registering targets: a backend that looks green
on every configuration anyone runs is not the same as a correct backend.

## Scope — 8 assertions across 6 files

```
debug/rainbow.glsl:132, :139
function/declare-prototype.glsl:40
function/param-default-in.glsl:85
function/param-in.glsl:39, :97
function/return-array.glsl:73
function/return-matrix.glsl:103
```

The cluster is coherent: **value parameters and aggregate returns**. Two of the
six are literally named for parameter passing; the other four return an array,
a matrix, or (in `rainbow`) a vec through a helper.

## Where to look first

M7 D1/D2: floats cross every boundary in **address** registers as raw bit
patterns, and *lowering* emits a `wfr` (AR→FR) at function entry to move each
parameter into the float file. The parameter's vreg is therefore integer-class
at the boundary and float-class inside the body.

A write to that parameter has to land in the FR copy that subsequent reads see.
If the two frontends allocate parameter vregs differently — and they do; the Lp
frontend resolves and numbers differently from Naga, which is why
`rv32lpn.q32` and `xtn.q32` have visibly different `unsupported` counts on the
same corpus — then one of them can produce a shape where the write goes to the
AR-side vreg and the read comes from the FR-side one. The symptom (reads see
the original argument) matches that exactly.

The nearest prior art is
`docs/defects/2026-07-30-xtensa-call-argument-clobber.md`, also
`config-masked-defect`, also a case where rv32 was correct only because its
argument registers and allocatable pool happened to be disjoint where Xtensa's
overlap.

## Disposition in the corpus

The 8 assertions are `@broken(xtlpn.f32)` with a reason line naming this file —
**not** `@unsupported`. The distinction is load-bearing: `@unsupported` means
"this target does not do this", which would be a lie, and would leave the
corpus green while the compiler is wrong. `@broken` means "this is a bug we
have written down", and it is the annotation to delete when this is fixed.

`xtn.f32` is unaffected and is **850/850 with no `@broken` of its own**.

## Why it is not fixed here

M8's brief is explicit: a real codegen bug found by triage is "a finding to
report and fix in its own change, not something to bury under an annotation".
Fixing it means touching `lower_f32`'s parameter handling, which is M7 surface,
with its own validation across all four Xtensa targets plus a silicon re-run.
