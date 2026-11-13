---
status: fixed
found: 2026-11-12      # how: lps-probe's diagnostic tests, once the probe compiled through lps-glsl
fixed: 2026-11-12
area: lps-glsl (the device's GLSL front end) — global initializer parsing
class: untested-path
---
# lps-glsl panics on an empty global initializer

**Symptom** — `float bad = ;` at global scope panicked ("body parser bump
called at end of token stream") instead of reporting an error. The rv32
firmwares are abort-tier, so a half-typed global in a pushed shader would
reset a board; in Studio's wasm (the shader agent's probe) it was a panic in
the tab.

**Why it hid** — a global's initializer is parsed from its own token slice,
and only that path can hand the expression parser an empty slice; inside a
function body the statement's `;` is always there to stop on. The corpus's
error tests had no empty global initializer. The probe's own tests did, but
they compiled through naga until 2026-11-12.

**Fix** — `parse_expr_tokens` reports "expected expression, found ;" at the
initializer for an empty slice, and `parse_primary` refuses to read past the
end of its tokens (`lps-glsl` unit test
`an_empty_global_initializer_is_a_diagnostic_not_a_panic`).
