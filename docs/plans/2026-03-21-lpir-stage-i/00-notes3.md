# LPIR Stage I — Pre-spec review notes (00-notes3)

Final loose ends before writing spec chapters. Each is a small clarification
or gap spotted during the plan review pass.

Work through **one section at a time**: discuss, decide, fold into plan docs.

---

## 1. Grammar: multi-return type and assignment syntax

**Context**
The grammar sketch in `05-text-format-grammar.md` defines
`type = "f32" | "i32"`, but multi-return functions use `-> (f32, f32, f32)`
and multi-return calls use `v4:f32, v5:f32, v6:f32 = call @fn(v0)`.
Neither production is in the grammar.

**Analysis**
Not a design question — multi-return is decided. But the grammar chapter
needs explicit productions for tuple return types and multi-assignment.
Without them, a parser implementer has to guess.

**Suggested answer**
Add to the grammar sketch:
- `return_type = type | "(" type { "," type } ")"` — scalar or tuple.
- `assignment = vreg_list "=" op` where `vreg_list` is one or more
  comma-separated `vreg_def`.
- Document that `call` is the only op that produces multi-return.

**Decision**:

Add to the grammar sketch in `05-text-format-grammar.md`:
- `return_type = type | "(" type { "," type } ")"` — scalar or tuple.
- `assignment = vreg_list "=" op` where
  `vreg_list = vreg_def { "," vreg_def }`.
- Only `call` produces multiple results; multi-assignment with any other
  op is a parse error.

---

## 2. Module versioning / text format header

**Context**
The text format has no module header or version marker. If the format
evolves incompatibly, parsers have no way to detect "this is v2 syntax."

**Analysis**
Not critical for v1 (there's only one version). But a one-line convention
now avoids a painful retrofit later.

**Suggested answer**
Reserve a comment convention: `;; lpir v1` as the first line. The parser
ignores it (it's a comment), but tooling can use it for version detection.
Alternatively, a `module` keyword with version: `module v1 { ... }`.
Either way, mention in "Future Extensions" that the format may gain a
version header.

**Decision**:

Not a concern. The text format is for debugging/testing/development, not
interchange. Generators may include `;; lpir v1` as a courtesy comment
but parsers are not required to check it. No grammar change needed. If a
breaking format change ever happens, the parser can handle it ad hoc.

---

## 3. GLSL `for`/`while`/`do-while` → LPIR `loop` mapping

**Context**
The spec covers `loop` + `br_if_not`, which is what Naga's
`Statement::Loop` maps to. But GLSL has `for`, `while`, and `do-while` —
all of which Naga funnels through `Statement::Loop { body, continuing,
break_if }`. A reader of the GLSL mapping chapter might wonder where
`for` went.

**Analysis**
Purely a documentation gap. The lowering is straightforward (Naga handles
it), but the mapping chapter should make this explicit.

**Suggested answer**
In `08-glsl-mapping.md`, add a note: "GLSL `for`, `while`, and `do-while`
all lower through Naga's `Statement::Loop` → LPIR `loop`. The `continuing`
block maps to ops before `br_if_not`; loop initializers are emitted before
the `loop`; increment expressions are part of `continuing`."

**Decision**:

Add a note in `04-import-modules-and-mapping.md` (statement mapping section) and
plan for `08-glsl-mapping.md`: "GLSL `for`, `while`, and `do-while` all
lower through Naga `Statement::Loop` → LPIR `loop`. Loop initializers emit
before the `loop`; the `continuing` block maps to ops before `br_if_not`;
increment expressions are part of `continuing`."

---

## 4. `switch` lowering strategy for v1

**Context**
`Statement::Switch` is listed as not-handled in `00-notes.md` and deferred
in the future extensions section (`05-text-format-grammar.md`). But if any
user shader uses `switch`, Naga will produce `Statement::Switch` and the
lowering must deal with it.

**Analysis**
Two options for v1:
- **(a)** Lower `switch` to an `if`/`else` chain in the Naga → LPIR
  lowering. No new IR op needed. Performance is fine for small case counts
  (typical in shaders).
- **(b)** Reject `switch` in the lowering with a clear error. Add it later
  when a `switch` op is designed (mapping to WASM `br_table`).

Option (a) is low-risk and means the lowering can accept more GLSL without
a new op. Option (b) is simpler but may surprise users.

**Suggested answer**
(a) — lower to `if`/`else` chain for v1. Reserve `switch` op for future.
Document in the mapping chapter that `Statement::Switch` is lowered to
cascaded `if`/`else` with `ieq` comparisons.

**Decision**:

Add `switch` as a first-class structured control flow op in LPIR. Lowering
to `if`/`else` chains destroys information — both backends have efficient
multi-way branch support (WASM `br_table`, Cranelift `Switch` utility) and
the IR should preserve the "multi-way branch on a single selector" intent.

Semantics:
- Selector must be `i32`.
- Case values are integer constants; duplicates are an error.
- **No fall-through**: each case arm is independent. Naga `fall_through:
  true` cases are merged in the Naga → LPIR lowering.
- `default` is optional. If absent and no case matches, control falls to
  after the switch.
- `break`/`continue` inside case bodies target enclosing `loop`s, not the
  switch (switch is not a loop construct).

Emitter lowering:
- WASM: nested `block`s + `br_table` for dense cases; `if`/`else` chain
  fallback for sparse cases (emitter optimization detail).
- Cranelift: `cranelift_frontend::Switch` (automatically picks jump table
  vs binary search based on density).

Naga mapping: `Statement::Switch { selector, cases }` → LPIR `switch`.
`SwitchCase { value: I32(n) }` → `case n { ... }`;
`SwitchValue::Default` → `default { ... }`.

Update: `00-design.md` (op diagram, control flow, well-formedness),
`03-control-flow.md`, `05-text-format-grammar.md`, `04-import-modules-and-mapping.md`.

---

## Summary checklist

- [x] Multi-return grammar productions added to `05-text-format-grammar.md`
- [x] Module versioning convention decided — not a concern, optional comment
- [x] `for`/`while`/`do-while` mapping note planned for `08-glsl-mapping.md`
- [x] `switch` — first-class control flow op, not lowered to `if`/`else`
