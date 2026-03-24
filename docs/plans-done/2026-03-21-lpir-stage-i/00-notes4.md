# LPIR Stage I — Pre-spec review notes (00-notes4)

Rethinking the `mathcall` mechanism. This is a significant design change
that affects the op set, call conventions, grammar, and mapping chapters.

---

## 1. Replace `mathcall` with module-qualified imports

**Problem**

The `mathcall` design implies a single, closed `MathFunc` enum in the IR
spec that must enumerate every external function we ever want to support.
This has several issues:

- **Closed set**: adding a function (e.g., a new Lygia builtin) requires
  changing the IR spec, not just configuring an emitter.
- **All-or-nothing**: an emitter must implement the whole `MathFunc` set
  or error. No way to express "this context provides sin/cos but not fma."
- **False unity**: `fsin` (standard math, every backend inlines it),
  `__lp_q32_add` (Q32-specific), and LPFX/Lygia builtins are fundamentally
  different things with different availability and linking strategies.
- **Testing friction**: testing basic LPIR control flow + arithmetic
  shouldn't require stubbing 40+ math functions.

**Decision**

Remove `mathcall` as a separate op and `MathFunc` as an IR-level enum.
All external functionality uses **module-qualified imports** resolved via
`call` — the same mechanism already used for LPFX and Q32 builtins.

Syntax:

```
; Module-qualified imports — module name is part of the function name
import @std.math::fsin(f32) -> f32
import @std.math::fcos(f32) -> f32
import @std.math::fmin(f32, f32) -> f32
import @lpfx::noise3(i32, i32, i32, i32) -> (i32, i32, i32)
import @lp.q32::q32_add(i32, i32) -> i32

; Local functions — no namespace qualifier
func @my_helper(v0:f32) -> f32 { ... }
entry func @shader_main(v0:f32) -> f32 { ... }

; Call sites use the full qualified name for imports
func @example(v0:f32) -> f32 {
  v1:f32 = call @std.math::fsin(v0)
  v2:f32 = call @my_helper(v1)
  return v2
}
```

The `::` separates module from function name. The parser structurally
distinguishes imported calls (`@module::name`) from local calls (`@name`).

**Emitter contract**

The emitter is configured with **providers** for import modules:

- `"std.math"` provider: WASM → browser libm imports; Cranelift → libcalls
  or intrinsics.
- `"lp.q32"` provider: only available in Q32 mode; provides fixed-point
  math functions.
- `"lpfx"` provider: Lygia builtins, only if configured.

If a module is required by the IR but no provider is configured, the
emitter returns an error. Signature mismatches are also errors.

**What the spec defines**

- The **mechanism**: `import @module::name(sig)` declarations, `call
  @module::name(args)` at call sites. Single `call` op for everything.
- **Well-known module catalogs** as reference documentation (e.g.,
  `std.math` lists fsin, fcos, etc. with signatures and semantic notes).
  These are reference catalogs, not closed enums — a module can grow
  without IR spec changes.
- **Rules**: all imports must be resolved by configured providers;
  unresolved module → emitter error; signature mismatch → emitter error.

**Naga → LPIR lowering**

The lowering knows the standard module names and emits import declarations
for the functions it needs:

- `Expression::Math { fun: Sin, .. }` → `import @std.math::fsin(f32) -> f32`
  + `call @std.math::fsin(v0)`.
- Q32 builtins → `import @lp.q32::...` declarations.
- LPFX calls → `import @lpfx::...` declarations.

**Benefits**

- **Open-ended**: new modules don't require IR spec changes.
- **Context-aware**: Q32 mode configures `lp.q32` provider; f32 mode
  doesn't. LPFX available only when configured.
- **Testable**: test harness provides only the modules it needs (or none
  for pure arithmetic tests).
- **Single calling mechanism**: `call` for everything — no `mathcall`
  vs `call` distinction.
- **Namespace safety**: `@std.math::fsin` can't conflict with
  `@lpfx::fsin` or a local `@fsin`.

**What changes in the plan**

- `00-design.md`: remove MathCall op category from diagram; update call
  conventions, examples, and GLSL mapping table.
- `01-type-system-and-core-ops.md`: remove `fmod` mathcall reference.
- `02-memory-and-calls.md`: update import syntax to module-qualified;
  update call examples.
- `04-mathcall-and-mapping.md`: rename/restructure — mathcall mechanism
  becomes import module mechanism; MathFunc table becomes `std.math`
  catalog; semantic precision section stays.
- `05-text-format-grammar.md`: remove `mathcall` keyword; update import
  and call grammar; add `::` to lexical rules.
- `06-cleanup.md`: update consistency checklist.
- `00-notes2.md` §5: annotate that the MathCall decision has been
  superseded by module-qualified imports.
- Spec file structure: rename `06-mathcall.md` to `06-import-modules.md`.

---

## Summary checklist

- [x] `mathcall` replaced with module-qualified imports (`@module::name`)
