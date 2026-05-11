# Phase 5 Design: Vectors

Plan reference: `2026-03-17-glsl-wasm-part-iii.md` Phase 5

## Goals

1. Extend LocalInfo with component count; allocate multiple locals for vec2/3/4
2. Vector constructors (vec2, vec3, vec4, ivec*, uvec*, bvec*)
3. Component access (`.x`, `.y`, `.z`, `.w`)
4. Swizzle (`.xy`, `.rgb`, `.xyzw`)
5. Vector variable load/store
6. Vector arithmetic (component-wise)
7. Scalar-vector promotion
8. Vector assignment and compound assignment
9. Vector return (WASM multi-value)
10. Vector parameters
11. Vector comparison (`==`, `!=`)

---

## 1. Multi-local representation

**LocalInfo change:**
```rust
pub struct LocalInfo {
    pub base_index: u32,      // first local index
    pub ty: Type,
    pub component_count: u32,  // 1 for scalar, 2-4 for vector
}
```

**add_local:** For vec2, allocate 2 consecutive locals; vec3 → 3; vec4 → 4. Return base_index.

**Backward compatibility:** Scalars have component_count=1, base_index=index. Existing `index` field can be `base_index` for scalars.

---

## 2. Vector constructors

**vec2(x, y):** emit x, emit y. Stack: [x, y]. For storage: local.set for each component.

**vec3(scalar):** emit scalar 3 times. Stack: [s, s, s].

**vec3(vec2, scalar):** emit vec2 comps, emit scalar. Stack: [v0, v1, s].

**vec4(vec3, scalar):** similar.

**Type coercion:** Each component may need coercion to base type. Use existing coercion for scalar.

---

## 3. Component access

**Expr::Dot(base, field, _):** Parse field name (".x", ".y", etc.). Map to offset 0..3. Emit `local.get(base_index + offset)`.

**Swizzle:** ".xy" → get 0, get 1. ".zyx" → get 2, get 1, get 0. Parse swizzle string to indices, emit local.get for each.

**Duplicate check:** Swizzles in assignment LHS cannot have duplicates (e.g. `.xx`). Phase 2 Cranelift checks this.

---

## 4. Vector load/store

**Load:** local.get for each component. Order: x, y, z, w.

**Store:** local.set for each component. For assignment to whole vector: emit rhs (N values), local.set for each.

---

## 5. Vector arithmetic

**vec + vec:** Component-wise. For each i: get a_i, get b_i, add, set result_i. Or: emit all a components, emit all b components, then for each pair emit op. Stack management: need to emit in order. For N=3: [a0,a1,a2,b0,b1,b2]. Then add: a0+b0, a1+b1, a2+b2. Use locals as temps if stack gets complex.

**Pattern:** For binary vec op, use temp locals: store lhs in temps, emit rhs, for each component: get lhs_i, get rhs_i (or it's on stack), op, set result. Simpler: emit lhs (all components pushed), emit rhs (all components pushed). Stack: [a0,a1,a2,b0,b1,b2]. We need (a0+b0, a1+b1, a2+b2). Wasm can't easily do this with stack alone. Use locals.

**Practical approach:** For each component i: local.get a_i, local.get b_i, op. Result stays on stack... but we need 3 results. So we need to store each result in a temp, then load for return. Or use a result vector's locals and set each. For rvalue, we push N values. So: allocate temp locals for result, for each i: get a_i, get b_i, add, set result_i. Then for each i: get result_i (to produce rvalue on stack). Or: just push as we go. After loop: stack has [r0, r1, r2]. That's the correct order for multi-value.

---

## 6. Scalar-vector promotion

**scalar * vec:** Replicate scalar N times, then component-wise mul. Or: for each i: get scalar, get vec_i, mul. Emit scalar once, then for each component: stack has [scalar], get vec_i, mul. But we'd need to duplicate scalar. So: emit scalar, store in temp. For each i: get temp, get vec_i, mul. Push results.

**vec * scalar:** Symmetric.

---

## 7. Vector assignment

**Simple:** rhs produces N values. lhs is variable with N locals. local.set for each (reverse order: last value goes to last local, so first value = first component). Actually stack: first pushed = bottom. Popping: last pushed = first popped. So for [r0, r1, r2] we'd pop r2, r1, r0. local.set idx+2, local.set idx+1, local.set idx+0. So we need to set in reverse order of push. Or: use local.tee for the first and local.set for the rest? Easier: pop each and set. local.set pops one value. So we need N local.sets. Order: top of stack = last component. So local.set idx+(N-1) first, then idx+(N-2), ... idx+0. That means we set from last component to first. Correct.

---

## 8. Vector return

**WASM multi-value:** Function type has multiple results: `(result i32) (result i32) (result i32)` for vec3. Return instruction pushes all. Emit N values, then `return`. wasm-encoder: `instr.return_()` pops nothing in multi-value; the values are already on stack from the block. Need to verify how wasm-encoder handles multi-value return.

---

## 9. Vector parameters

**Function signature:** vec3 param = 3 i32 params. Caller pushes 3 values. Callee receives them as locals (params). Parameter allocation: first param = local 0, etc. For vec3 p, we have 3 param indices. In add_local, params are pre-allocated. For a func with (vec3 a, vec2 b), we have 5 param locals. LocalInfo for a: base_index=0, component_count=3. For b: base_index=3, component_count=2.

---

## 10. Vector comparison

**vec == vec:** Component-wise eq, reduce with &&. (a0==b0) && (a1==b1) && (a2==b2). Emit: eq for each, then && chain.

**vec != vec:** !(vec == vec) or component-wise ne, reduce with ||. Actually GLSL: != is aggregate inequality. So !(a==b). Emit == then i32.eqz.

---

## File change summary

| File | Changes |
|------|---------|
| `codegen/context.rs` | LocalInfo base_index, component_count; add_local for vectors |
| `codegen/expr/constructor.rs` | emit_vector_constructor |
| `codegen/expr/component.rs` | New: emit_dot (component access, swizzle) |
| `codegen/expr/variable.rs` | Vector load (multiple local.get) |
| `codegen/expr/binary.rs` | Vector arithmetic dispatch |
| `codegen/expr/assignment.rs` | Vector assignment (component-wise set) |
| `codegen/stmt/return_.rs` | Multi-value return |
| `types.rs` | glsl_type_to_wasm for vectors (returns N ValTypes) |
| `codegen/mod.rs` | Type section: vector params/results → multiple ValTypes |

---

## Validation

- vec2/3/4 tests, ivec, uvec, bvec
- Vector constructors, swizzles, arithmetic, comparison
