---
status: fixed
found: 2026-11-12      # how: planning the switch to one GLSL frontend; lps-glsl run on the interp tier failed every unit that pulled in the canonical `lpfn_*` sources
fixed: 2026-11-12
area: lps-glsl (the device's GLSL front end) — lexer, parser, type checker, lowering
class: untested-path
---
# lps-glsl has no bitwise operators

**Symptom** — a shader using `&`, `|`, `~`, `<<` or `>>` (or `&=`, `|=`,
`^=`, `<<=`, `>>=`) failed to compile on every board: `unexpected
character` at `|`, `&` or `~`, or a parse error at `^` and the shifts.
`x | 1` in a pattern was a compile error on a C6 while Studio's in-browser
simulator, then on the naga front end, rendered it.

**Why it hid** — no filetest used a bitwise operator, and no shader in
`catalog/` or `projects/` did either, outside comments. Every corpus target
but `rv32lpn.*` compiled through naga, which has them. The first thing to
use them through lps-glsl was the canonical `lpfn_*` sources (`lp_hash`),
which only the interp tier compiles as GLSL: running that tier on lps-glsl
while planning the one-frontend switch failed 18 files at the first `|`.

**Fix** — the lexer reads the single `&`, `|`, `~`, the shifts and their
compound assignments; the parser places them at GLSL ES 3.00 §5.1
precedence (`|` < `^` < `&` < equality < relational < shift < additive);
the type checker takes int/uint scalars and vectors only (a shift keeps its
left operand's type and takes a count of either signedness, scalar or of the
same width); the lowering emits LPIR's `Iand`/`Ior`/`Ixor`/`Ibnot`/`Ishl`
and `IshrS` or `IshrU` by the left operand's signedness; literal operands
fold. Proved by `lp-shader/lps-filetests/filetests/operators/bitwise-*.glsl`
(49 directives), so the corpus can no longer be blind to them.
