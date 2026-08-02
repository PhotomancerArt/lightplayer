---
status: fixed
found: 2026-08-01      # how: M8 registering xtn.f32 / xtlpn.f32 and running the corpus
fixed: this change
area: lp-shader/lpvm-native (Xtensa f32 lowering), lps-glsl frontend
class: config-masked-defect
related:
  - docs/adr/2026-08-01-float-mode-as-a-compiler-parameter.md
  - docs/defects/2026-07-30-xtensa-call-argument-clobber.md
---
# `xtlpn.f32` loses writes to value parameters — Lp frontend **and** Xtensa **and** f32, all three required

**Symptom** — a function that modifies its own `in` parameter and returns it
returned the *unmodified argument*:

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
| `xtn.f32` | Naga | Xtensa | F32 | **passed** |
| `rv32lpn.f32` | Lp | rv32 | F32 | **passed** |
| `xtlpn.q32` | Lp | Xtensa | Q32 | **passed** |
| **`xtlpn.f32`** | **Lp** | **Xtensa** | **F32** | **FAILED** |

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

## Root cause — a float parameter had two homes, and the stale one stayed readable

M7 D1/D2: floats cross every boundary in **address** registers as raw bit
patterns, and *lowering* emits a `wfr` (AR→FR) at function entry to move each
parameter into the float file. A vreg has one register class for its whole life,
so a float parameter cannot be both the precolored AR the ABI hands it in and
the FR the body computes with. `lower_f32::float_vreg` therefore gives every
float parameter a second vreg — the **shadow**, in a reserved block — and the
entry `wfr` fills it.

Two homes, and the invariant that keeps them straight was never stated: *after
entry, only the shadow is current.* `lower_f32::word_operand` — the call-argument
and return boundary — shortcut a float parameter straight back to its incoming
address register, reasoning that "it never left the AR, so re-deriving it from
the shadow would be a pointless round trip". True only of a parameter nobody
writes to.

**LPIR is not SSA**, and a GLSL value parameter is an ordinary mutable local.
The `lps-glsl` frontend lowers `x = x + 1.0` as a redefinition of the
parameter's *own* vreg:

```
func @modify_and_return(v1:f32) -> f32 {
  v3:f32 = fconst.f32 1.0
  v4:f32 = fadd v1, v3
  v1 = copy v4          ← the write: lands in v1's FR shadow
  return v1             ← the read: took the shortcut to v1's incoming AR
}
```

The write goes to the shadow (every float op writes through `float_vreg`); the
return read took the AR, which still held what the caller passed. Hence "reads
see the original argument", exactly.

Naga's frontend does not reuse the parameter vreg — it copies parameters into
fresh locals — so `xtn.f32` never builds this shape. rv32 f32 is soft-float: one
integer home, no shadow, nothing to go stale. Q32 floats are integers, likewise
one home. Three axes.

## Fix

`lower_f32::word_operand` no longer special-cases parameters: **after the entry
transfer a float parameter is read from the float file and nowhere else**, via
the same `rfr` as any other float value. Non-float values still pass through
untouched.

The rule is unconditional on purpose. The alternative — keep the shortcut but
gate it on "this parameter is never reassigned" — saves one instruction and
reintroduces the thing that failed here: a predicate about which values the fast
path is safe for. This version has no predicate to get wrong, and the AR side
now has no reader after entry at all.

Nor is it purely a cost. The shortcut kept the incoming argument register live
from entry all the way to whatever boundary used it; now it dies at the entry
`wfr`. On Xtensa the argument bank **is** the caller-saved half of the
allocatable pool — the register-overlap hazard behind
`2026-07-30-xtensa-call-argument-clobber.md` — so shortening that live range
relieves the pool rather than straining it.

## Regression coverage

- `lpvm-native/tests/xt_pipeline_f32.rs::a_reassigned_float_parameter_is_read_back_from_the_float_file`
  — end to end through the emulator, covering **both** boundaries: `bump`
  returns its reassigned parameter, `forward` passes its own reassigned
  parameter on as a call argument. Checked falsifiable: reverting the fix makes
  it fail `5.0 != 6.0` on the return boundary.
- `lower_f32::tests::a_float_parameter_at_a_boundary_reads_the_shadow` — the
  lowering-level claim, that the emitted transfer names the shadow and not the
  incoming AR.
- The 8 corpus assertions, with their `@broken(xtlpn.f32)` annotations deleted.

Validation, all four Xtensa targets at 850/850 files and 0 fail:

| Target | Before | After |
|---|---|---|
| `xtlpn.f32` | 6358/6358, **10** expected-failure | **6366/6366, 2** |
| `xtn.f32` | 6344/6344, 79 expected-failure | 6344/6344, 79 |
| `xtlpn.q32` | 6387/6387, 10 expected-failure | 6387/6387, 10 |
| `xtn.q32` | 6338/6338, 86 expected-failure | 6338/6338, 86 |

The 8 that moved are exactly the annotations deleted; the 2 left on `xtlpn.f32`
are an unrelated `@unimplemented` pair in `function/overload-local-duplicate.glsl`.
The Q32 pair is result-identical, as it must be by construction — `word_operand`
is unreachable unless `uses_hardware_fpu`, i.e. `FloatMode::F32` on an FPU
target — and was diffed run-against-run rather than argued. The default set
(`interp.f32`, `rv32c.q32`, `rv32lpn.q32`, `rv32n.q32`, `wasm.q32`) plus
`rv32lpn.f32` and `wasm.f32` are unchanged and green.

## Lesson

When a value gets a second home for a register-class reason, the rule saying
*which home is current, and from when* is part of the design rather than an
implementation detail — and the place it gets violated is a fast path that skips
the transfer. `word_operand`'s shortcut was justified in a doc comment ("a
pointless round trip") that was true of the only frontend it had ever run
against.

The sharper half: this is the 2026-07-30 Xtensa family one level up. Those were
*shared allocator code that rv32's register layout made unfalsifiable*; this is
*shared lowering code that Naga's vreg numbering made unfalsifiable*. The
masking axis moved from the ISA to the frontend and the detection mechanism did
not change: run the real combination as a target. `@broken` rather than
`@unsupported` is what kept it visible in the interval between finding and
fixing — a corpus that reads green while the compiler is wrong is worse than no
corpus.
