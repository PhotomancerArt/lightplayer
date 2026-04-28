# Filetest triage — unsupported on q32 (by design / until real f32 / IEEE host)

This list covers failures that are **expected to stay** marked unimplemented (or should move to `// @unsupported(target)`) while the product uses **q32 (or similar) fixed/scalar numerics** instead of true IEEE-754 `float` in the execution engine. All three backends (`rv32n`, `rv32c`, `wasm`) should stay aligned: if something is intrinsically “not possible on q32,” annotate all targets the same way.

**Legend:** *Decision* — use `Y` / *custom…* to track your choice.

## Real f32 bit reinterpret & dependent packing

| Failing tests (collapsed) | Targets | What fails | Suggested path forward | Decision |
|---------------------------|---------|------------|------------------------|----------|
| `builtins/common-floatbitstoint.glsl` (8 runs) | all | GLSL **parse rejects infinite float literals** (`1.0/0.0`) used in tests; `floatBitsToInt` is meaningless without real f32 | Keep `// @unimplemented` or add `// @unsupported(rv32n.q32)` (etc.); re-enable when a real-f32 path exists |  |
| `builtins/common-intbitstofloat.glsl` (7 of 8 runs) | all | **Inf/NaN literal** parse failures; large `int` literals limited by host pipeline (e.g. **16-bit eval**) — *see also [broken.md](./broken.md) for the literal-width bug* | Mark lines that need IEEE `intBitsToFloat` as **unsupported** on q32; fix literal width separately in broken |  |
| `builtins/pack-double.glsl`, `builtins/unpack-double.glsl` | all | Not implemented / needs double semantics | `// @unsupported` on q32 permanently unless doubles are a product goal |  |
| `builtins/pack-half.glsl`, `builtins/unpack-half.glsl` | all | Pack/unpack **half** (FP16) | `// @unsupported` on q32; optional host-only path later |  |
| `builtins/pack-unorm.glsl`, `builtins/unpack-unorm.glsl` | all | Unorm8 packing | Often tied to real float width; treat as **unsupported** on q32 until a spec’d fixed-point mapping exists |  |
| `builtins/common-frexp.glsl` | all | `Unknown function 'frexp'` (not lowered) | `// @unsupported` until `frexp` is defined for **q32 numerics** (non-IEEE) or real f32 exists |  |
| `builtins/common-modf.glsl` | all | `modf` not available | Same as `frexp` |  |

## Domain / edge tests that require infinities or NaN in the source

| Failing tests (collapsed) | Targets | What fails | Suggested path forward | Decision |
|---------------------------|---------|------------|------------------------|----------|
| `builtins/edge-exp-domain.glsl` | **wasm** (in snapshot) | `Float literal is infinite` at parse; remaining lines `@unsupported` | Per-line `// @unsupported(wasm.q32)`; same on rv32 when run |  |
| `builtins/edge-nan-inf-propagation.glsl` | wasm | `NaN` / `Inf` in source | `// @unsupported` all targets for q32 |  |
| `builtins/edge-trig-domain.glsl` | wasm | Same class | `// @unsupported` |  |

*Note: rv32 lines in the snapshot may show fewer of these as plain `compile-fail` with the same root cause: parser rejects non-finite literals.*

## Summary

- Prefer **`// @unsupported(<target>)`** (with a one-line reason in release notes) over leaving bare `// @unimplemented` for “never on q32” cases, so `LP_MARK_UNIMPLEMENTED` runs don’t keep churning the same files.
- When **real f32** (or a documented hybrid) lands, re-triage: bit casts, `frexp`/`modf`, and pack/unpack families become **broken/implemented**, not unsupported.

---

*Generated for triage on 2026-04-23. Sample runs used `scripts/glsl-filetests.sh`.*
