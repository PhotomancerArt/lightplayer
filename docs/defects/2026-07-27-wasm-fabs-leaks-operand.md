---
status: fixed
found: 2026-07-27      # how: live shader-agent session, bisected
fixed: this change
area: lp-shader/lpvm-wasm (Q32 WASM emission)
class: masked-invariant
related:
  - docs/defects/2026-07-27-cranelift-q32-rounding-runtime.md
---
# `abs()` in a fall-through branch failed to compile on the WASM backend

**Symptom** — A shader compiled fine under the probe oracle (`interp.f32`) but
the WASM backend rejected it:

```
shader WASM parse/validate failed: ...
Invalid input WebAssembly code: type mismatch: values remaining on stack at end of block
```

Chrome words the same failure "expected 0 elements on the stack for fallthru,
found 2". Bisection landed on `abs()` on a float inside the if-branch of a for
loop where the branch also stored into an array; the failure persisted when the
result was multiplied by zero, and `sqrt(x*x)` compiled where `abs(x)` did not.

**Root cause** — `lpvm_wasm::emit::q32::emit_q32_fabs` pushed `src` once *before*
emitting its own condition:

```rust
sink.local_get(src)          // <- leaked: nothing ever consumes this
    .local_get(src).i32_const(0).i32_lt_s().if_(t);
…
sink.local_set(dst);         // consumes only the `if`'s result
```

Every `abs` therefore left one i32 on the WASM operand stack. LPIR is a register
machine, so each op must be stack-neutral; this one was not.

**Why it hid for so long** — WASM validation goes *polymorphic* after `return`:
everything up to the implicit function `end` becomes unreachable and is never
type-checked. In a straight-line function that returns, the leak is invisible and
the module validates. It only trips validation where the enclosing block **falls
through** — a builtin call inside an `if` inside a loop. The count in the error
("found 2") is just how many leaking calls the branch happened to contain.

Nothing in the corpus put a builtin call in a fall-through branch, and every
`filetests/builtins/` case passes its arguments as *literals*, which fold at
compile time and never reach the backend emitter at all. `abs` on a float had no
filetest of its own.

Not related to the concurrent break/continue-in-nested-loops investigation: this
is a per-op stack imbalance, not structured-control-flow lowering. The
`emit::control` stack machinery is untouched.

**Fix** — dropped the spurious `local_get`. Only `emit_q32_fabs` was affected;
every other arm of `emit::ops` and `emit::q32` audited and found neutral.

**Regression coverage**

- `lpvm-wasm` unit test `emit::q32::tests::q32_helpers_are_stack_neutral` emits
  each Q32 helper into a function body whose implicit `end` is **reachable** (no
  trailing `return`) and validates it with `wasmparser`. Verified to fail with
  the original code and pass with the fix. This is the structural guard: it pins
  the invariant per helper instead of hoping a shader exercises it.
- `filetests/control/torture/intrin_*.glsl` — new generated axis: one file per
  builtin, each calling it inside an `if` inside a `for`, with the result stored
  to an array, stored through a swizzle, discarded, or negated in the other arm.
  Any non-neutral emitter fails to compile there.
- `filetests/control/edge_cases/builtin-call-in-loop-branch.glsl` — the bisected
  shape kept verbatim.
