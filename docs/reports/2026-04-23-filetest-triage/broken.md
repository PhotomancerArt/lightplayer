# Filetest triage — broken / should fix (compiler & backend parity)

Failures here are **not** “real float is missing” excuses: they are bugs, **missing features we intend to have on q32**, test expectation mistakes, or **wasm vs rv32** parity gaps. All three backends should match unless a row says otherwise.

The line list is from a **full-filetests snapshot** (2026-04-23). A few spot-checks differ now (e.g. `vec/uvec4/from-mixed.glsl` is **all pass** on `rv32n.q32` in this tree); the **wasm** column issues for `uvec* from-mixed` remain the cross-backend gap to fix.

**Legend:** *Decision* — `Y` to take the suggested fix, or your custom note.

## A. Resolver / front-end (GLSL): overloads, qualifiers, l-values

| Failing tests (collapsed) | Targets | What fails | Suggested path forward | Decision |
|---------------------------|---------|------------|------------------------|----------|
| `vec/bvec2|bvec3|bvec4/fn-mix.glsl` (6 runs each) | all | `Ambiguous best function for 'mix'` for `bvecN` (overlapping `mix` signatures) | Teach the resolver a **tie-break** (GLSL: `bool` selector branch vs lerp) or disambiguate in `lps-frontend` |  |
| `function/edge-const-out-error.glsl` (5) | all | `Expected Void, Struct or a type, found In` for `const in` parameter order | **Parser/validator**: accept GLSL 4.x `const` + `in` order on parameters |  |
| `function/edge-lvalue-out.glsl` (7) | all | e.g. `out` to **non-local** lvalue (array element / deeper access) not supported | Extend **lvalue** / out lowering for the supported aggregate model (pointer ABI / sret) |  |
| `function/edge-return-type-match.glsl` (10) | all | e.g. array assignment from expressions not in the supported subset | Align **array** assignment rules with the implemented aggregate story or error earlier with a clear diagnostic |  |
| `function/declare-prototype.glsl` (1 of 4) | all | `// run: test_declare_prototype_vector(vec4(1.0), vec4(2.0))` — harness: **`failed to parse function arguments: ["vec4(1.0)", "vec4(2.0)"]`** | **Extend lps-filetests** to parse / pass **vector-typed** call arguments, or add an alternate `// run` form |  |
| `function/overload-same-name.glsl` (2 of 7) | all | (1) `test_overload_same_name()` `~= 21.0` — **wrong** sum across `float`/`int`/`uint` overloads. (2) `test_overload_parameter_count() ~= 12.0` — **actual 10.0**; *file comment* flags **mixed-arity calls in one function** as a known front-end/ABI bug | Disambiguation / call lowering for **multiple overloads in one expression**; fix **mixed-arity** chain (`sum_1+sum_2+sum_3`) |  |
| `function/call-order.glsl` (1 of 6) | **rv32n** in snapshot (only target marked) | `test_call_order_left_to_right`: **`InvalidMemoryAccess`** in emu, expected `6.0` | **Three-arg** call: fix arg eval order and/or **global** + stack frame for re-entrancy; verify on wasm |  |

## B. q32 numerics: wasm **≠** rv32 (must converge)

| Failing tests (collapsed) | Targets | What fails | Suggested path forward | Decision |
|---------------------------|---------|------------|------------------------|----------|
| `scalar/float/from-uint.glsl` | **wasm** in snapshot; **rv32 passes** in re-check | Large `uint` → `float` off by ULP in high bits | Unify **uniform scalar cast** path in the wasm code generator / LPVM with rv32n |  |
| `scalar/int/from-float.glsl` (3) | wasm | `int(negative float)` off by 1 (truncate-toward-zero) | Fix wasm `ftoi` to match GLSL / rv32 (truncate toward zero) on q32 |  |
| `scalar/uint/from-float.glsl` (3) | wasm | Negative `float` → `uint` not clamping to 0; shows `0xFFxx` **patterns** | Fix wasm: **NaN/neg → 0u** (GLSL) |  |
| `vec/uvec2|3|4/from-mixed.glsl` (1 each) | **wasm** | `vecN` → `uvecN` / mixed ctor: e.g. second component `65533u` instead of `0u` | Same as scalar: wrong **f32-to-u32** lowering on wasm; align with rv32 |  |

*Current repo check: `vec/uvec4/from-mixed.glsl` and `scalar/float/from-uint` are **all green** on `rv32n.q32` — treat wasm as the outlier to fix.*

## C. Uniform & globals (memory / initialization)

| Failing tests (collapsed) | Targets | What fails | Suggested path forward | Decision |
|---------------------------|---------|------------|------------------------|----------|
| `global/type-array.glsl` (8) | all | `unsupported statement: store through Access: unsupported base` (global / typed array) | Complete **store-through-pointer** (or array slot write) for uniform-backed / global array cells |  |
| `global/forward-reference.glsl` (1 of 5) | all | `test_forward_reference_mat` — forward-declared `mat3` reads as **all zeros** | Init / fixup pass for **globals** with forward references (order-dependent) |  |
| `uniform/defaults.glsl` (1 of 6) | all | `test_initialize_uniform_usage` expected `2.0`, got `1.0` | Trace uniform defaults + `float(int)` + `vec2` add in one expression; likely **one uniform slot** wrong |  |
| `uniform/no-init.glsl`, `uniform/pipeline.glsl`, `uniform/readonly.glsl`, `uniform/write-error.glsl` (1 each) | all | Residual | Same category: **uniform read path** and readonly checks |  |

## D. Matrix stack (same pattern across `mat2` / `mat3` / `mat4`)

*The same **file name pattern** per size (`op-multiply`, `op-assign`, `op-add-assign`, `op-multiply-assign`, `op-subtract-assign`, `from-scalar|matrix`, `fn-determinant`)* shows **one or a few** failing lines per file in the snapshot; you do not need to triage every line — root causes are shared.

| Failing groups | Targets | What fails | Suggested path forward | Decision |
|----------------|---------|------------|------------------------|----------|
| `matrix/mat2|mat3|mat4/op-multiply.glsl` (4 fails each) | all | e.g. **associative** `mat * mat * mat` result wrong; actual does not match expected | Fix **matmul convention** (row vs column) consistently in multiply lowering + `linear algebra` tests |  |
| `matrix/mat2|mat3|mat4/op-assign.glsl`, `op-add-assign.glsl`, `op-multiply-assign.glsl`, `op-subtract-assign.glsl` (1–2 each) | all | Chained / compound assignment vs plain multiply | Once matmul is fixed, re-run; then fix **compound assignment** ordering if any remain |  |
| `matrix/mat2|mat3|mat4/op-multiply*.glsl` (subset) | all | `mat3` subtract-assign had 2 unimplemented in **mat3** in snapshot | Same as D |  |
| `matrix/mat4/from-scalar.glsl`, `from-matrix.glsl` (1 each) | all | Residual | Constructor lowering after core matmul fix |  |
| `builtins/matrix-outerproduct.glsl` (20) | all | Compile: bad / unsupported `outerProduct` type (e.g. `matrix` vs vector layout) | Implement **outerProduct** in terms of `vec ⊗ vec` with current layout |  |
| `builtins/matrix-transpose.glsl` (20) | all | **Transpose** not implemented in lowering | Add **transpose** for mat2/3/4 in lpir / backend |  |
| `builtins/matrix-inverse.glsl` (3 of 16) | all | e.g. inverse of **diagonal** scale matrices wrong vs expected | After matmul+layout are correct, fix **inverse** (often shares helpers with determinant) |  |
| `builtins/matrix-determinant.glsl` (1 of 21) | all | `test_determinant_mat4_negative`: expected **-1.0**, actual **1.0** for `diag(-1,…,-1)` | **Verify** GLSL `mat4` column layout: $\det$ of four **-1**s is **+1** — likely **test expectation** should be `1.0` |  |

## E. Integer intrinsics: wide ops, bitfield, `findMSB`, `bitCount`

| Failing tests (collapsed) | Targets | What fails | Suggested path forward | Decision |
|---------------------------|---------|------------|------------------------|----------|
| `builtins/integer-bitfieldextract.glsl`, `integer-bitfieldinsert.glsl` (6 each) | all | e.g. `Math::ExtractBits` / insert not lowered | Add scalar + vector **bitfield** lowering for q32 (no IEEE requirement) |  |
| `builtins/integer-imulextended.glsl`, `integer-umulextended.glsl` (4 each) | all | 64-bit result intrinsics not implemented | Lower to **mul + mulh**-style steps on 32-bit |  |
| `builtins/integer-uaddcarry.glsl`, `integer-usubborrow.glsl` (4–5) | all | Same | Emit **add/sub + carry** or reject with clear error; prefer emit |  |
| `builtins/integer-bitcount.glsl` (4 of 9) | all | **Harness type mismatch** (`0` vs `0u` style) for `bitCount` return | **Fix filetest** expectations to match uint vs int, or fix printer |  |
| `builtins/integer-findmsb.glsl` (2 of 10) | all | `findMSB` wrong for **negative** and **0x8000_0000** edge | Adjust **MSB** definition for sign vs GLSL; fix 31 vs 30 cases |  |
| `builtins/common-roundeven.glsl` (1 of 9) | all | e.g. `roundEven` on **one component** of `vec4` (half-integer toward negative) | **roundEven** on q32: match spec for .5 ulp tie |  |
| `builtins/common-intbitstofloat.glsl` — **literal width** part | all | e.g. `intBitsToFloat(1065353216)` → `32767.0` (16-bit int parse) | **Const-eval / lexer**: promote large decimal literals to 32-bit where GLSL requires |  |

*The “parse infinite literal” **parts** of `intbitstofloat` belong in [unsupported.md](./unsupported.md).*

## F. Control flow & ternary

| Failing tests (collapsed) | Targets | What fails | Suggested path forward | Decision |
|---------------------------|---------|------------|------------------------|----------|
| `control/ternary/types.glsl` (1 of 12) | all | e.g. `test_ternary_struct_complex` expected `15`, got `14` | Ternary on **struct** / nested aggregate: one slot wrong in `phi` or copy |  |
| `control/edge_cases/loop-expression-scope.glsl` (1 of 6) | all | `for` loop: variable incremented in body and step; final value | Clarify GLSL for-loop scoping; fix lowering so **for-init + step** match spec |  |

## G. Function parameters & builtins “still missing” (non-f32)

| Failing tests (collapsed) | Targets | What fails | Suggested path forward | Decision |
|---------------------------|---------|------------|------------------------|----------|
| `function/param-default-in.glsl` (1 of 10) | all | `test_param_default_vector`: expected `~10.0`, got `~7.21` | `length(v1+v2)` for `(1,2)+(3,4)` is $√52≈7.21$ — **expected 10.0 is inconsistent with GLSL**; **fix the test** to $√52$ or change inputs to yield length 10 |  |
| `global-future/*.glsl` (all compile-fail) | all | `buffer` / `shared` / `in` as global — **not in current product** | Leave compile-fail or gate behind feature; not q32 |  |
| `builtins/integer-*.glsl` | — | covered in E |  |  |

## H. Suggested course forward (phased)

**Phase 1 — Parity & obvious test fixes (fast wins)**  
- Fix **wasm** scalar / vector casts to match `rv32` (section B).  
- Correct **`param-default-in`** and **`matrix-determinant` mat4 negative** if tests are wrong.  
- Fix **bitcount** expected types in the filetest (section E).  

**Phase 2 — Front-end & overloads**  
- `mix` for **`bvecN`**, `const`/`in` parameter order, array assignment / out rules (section A).  

**Phase 3 — Matrix core**  
- Single **matmul + layout** fix, then `transpose`, `outerProduct`, `inverse`, remaining op-assign (section D).  

**Phase 4 — Memory model**  
- Global **typed array** stores, forward-ref globals (section C).  

**Phase 5 — Integer intrinsics**  
- **Bitfield**, extended mul, carry/borrow, **findMSB** edges, **roundEven** (section E).  

**Phase 6 — Control**  
- Ternary on **aggregates**, for-loop scoping (section F).  

**Phase 7 — “Real float”**  
- Revisit [unsupported.md](./unsupported.md); promote rows to “implement” or keep `@unsupported` with docs.

---

*Generated 2026-04-23. Representative commands: `scripts/glsl-filetests.sh <file> -t rv32n.q32`, `DEBUG=1` for a single `// run` line.*
