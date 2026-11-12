---
status: fixed
found: 2026-11-12      # how: the interp tier on lps-glsl, after the bitwise fix, stopped at `0xF8u` in the canonical lp_hash source
fixed: 2026-11-12
area: lps-glsl (the device's GLSL front end) — lexer and literal parsing
class: untested-path
related:
  - 2026-11-12-lps-glsl-has-no-bitwise-operators.ai.md
---
# lps-glsl has no hex or octal literals

**Symptom** — `0xF8u` lexed as `0` followed by the identifier `xF8u`, so a
shader with a hex literal failed on every board ("expected ), found xF8u").
A leading-zero literal such as `017` was read as decimal 17 where GLSL
means octal 15.

**Why it hid** — the same blind spot as the bitwise operators: the corpus
and the catalog wrote integers in decimal, and the only hex in a compiled
shader was in the canonical `lpfn_*` sources, which only the naga-based
interp tier compiled as GLSL.

**Fix** — the lexer reads `0x`/`0X` hex digits with an optional `u`, and
one function (`token::int_literal_value`) gives every literal site its
value by GLSL ES 3.00 §4.1.3: hexadecimal, octal for a leading `0`,
decimal otherwise. Proved by `filetests/scalar/int/literal-bases.glsl`.
