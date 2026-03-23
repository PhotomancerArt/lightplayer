# Phase 5: Text Format Grammar and Examples

## Scope

Write two spec chapters:
- `docs/lpir/07-text-format.md` — Formal (semi-formal) grammar
  definition, lexical rules, _imm syntax, comprehensive examples.
- `docs/lpir/09-future.md` — Reserved ops, vector types, planned
  extensions, forward-compatibility notes.

## Reminders

- This is a spec-writing phase, no Rust code.
- The grammar should be LL(1)-parseable and line-oriented.
- Examples should be self-contained and testable (when the parser exists).
- Cover every op and control flow construct at least once.

## Implementation details

### 1. Grammar Definition

Write a semi-formal EBNF-style grammar. The text format is line-oriented
and LL(1)-parseable.

Key grammar elements to define:

```
module      = { import | func | entry_func }
import      = "import" func_name "(" type_list ")" [ "->" return_type ]
func        = "func" func_name "(" param_list ")" [ "->" return_type ] "{" body "}"
entry_func  = "entry" "func" func_name "(" param_list ")" [ "->" return_type ] "{" body "}"
              ; at most one per module (runtime entry point)
body        = { slot_decl } { statement }
slot_decl   = "slot" slot_name "," integer   ; slot_name begins with "ss"
statement   = assignment | void_call | control_flow | switch_stmt
              | "return" [ vreg ] | "break" | "continue" | "br_if_not" vreg
switch_stmt = "switch" vreg "{" { switch_case } [ default_case ] "}"
switch_case = "case" integer "{" body "}"
default_case= "default" "{" body "}"
assignment  = vreg_list "=" op
vreg_list   = vreg_def { "," vreg_def }  ; multi-assign only valid with call
vreg_def    = vreg_name [ ":" type ]     ; type required on first definition
vreg_name   = "v" integer
slot_name   = "ss" integer
func_name   = "@" identifier
type        = "f32" | "i32"
return_type = type | "(" type { "," type } ")"   ; scalar or tuple
```

Define the complete production rules for:
- Module structure (imports, functions)
- Function signatures (params, return type)
- Slot declarations
- All op forms (binary, unary, const, cast, select, copy)
- MathCall syntax
- Memory ops (load, store, slot_addr, memcpy)
- Call syntax
- Control flow (if/else, loop, break, continue, br_if_not, switch, return)
- Comments (`;` to end of line)
- Literals (decimal int, hex int, float with decimal point, special
  float values: `inf`, `-inf`, `nan`)

### 2. Lexical Rules

Document:
- Whitespace: spaces and tabs, significant only as token separators.
- Line-oriented: one statement per line (control flow blocks span lines).
- Comments: `;` to end of line.
- Identifiers: `[a-zA-Z_][a-zA-Z0-9_]*`.
- VReg names: `v` followed by decimal digits.
- Slot names: `ss` followed by decimal digits.
- Function names: `@` followed by identifier.
- Integer literals: decimal (`42`, `-1`) or hex (`0xFF`).
- Float literals: decimal with point (`1.5`, `-0.0`, `0.0`), or special
  (`inf`, `-inf`, `nan`).
- Keywords: `func`, `entry`, `import`, `slot`, `if`, `else`, `loop`,
  `break`, `continue`, `return`, `br_if_not`, `switch`, `case`,
  `default`, `call`, `mathcall`, `select`, `copy`, `load`, `store`,
  `slot_addr`, `memcpy`, `f32`, `i32`.
- Typed constant syntax `iconst.i32` and `fconst.f32` are not single
  keywords; document them as dotted opcode names in the grammar (same
  pattern as other multi-token op spellings).

### 3. Immediate variants (`_imm`)

Document `_imm`-suffixed ops that take a literal immediate as the final
operand (CLIF-style), for example:

```
v2 = iadd_imm v1, 1
```

The immediate is part of the instruction; no separate VReg is produced
for that constant operand. Define the rules:
- Each `_imm` form lists which types and literal syntaxes are allowed for
  the immediate.
- Arbitrary ops do not accept `iconst.i32 …` or `fconst.f32 …` in operand
  position; use the corresponding `_imm` variant or a full assignment
  `v = iconst.i32 <value>` / `v = fconst.f32 <value>` when a VReg is needed.

Note: Decide which ops have `_imm` spellings vs requiring named constant
VRegs. Document the decision in the op reference.

### 4. Comprehensive Examples

Write complete, self-contained examples that cover every feature:

#### Example 1: Arithmetic and math builtins
A function that exercises all arithmetic ops and several mathcalls.

#### Example 2: All comparison operators
A function with every comparison op (float and integer, signed and unsigned).

#### Example 3: Casts
A function demonstrating every cast op.

#### Example 4: Control flow patterns
- Simple if
- If/else
- Loop with counter and br_if_not
- Nested loops
- Switch with cases and default
- Early return
- Loop with break (not br_if_not)

#### Example 5: Memory operations
- Slot declaration and slot_addr
- Load and store with various offsets
- Pointer arithmetic for dynamic array access
- memcpy

#### Example 6: Function calls
- Import declaration and call to imported function
- Local function definition and call
- Void function call
- Call with return value

#### Example 7: Realistic shader function
A scalarized version of a real shader pattern (e.g., smoothstep applied
to a vec3, with loop and conditional).

### 5. Future Extensions section

Brief section noting ops and features reserved for future work:

- Relational ops (`any`, `all`) — currently decomposed during scalarization.
  May be added as MathCall entries if needed for optimization.
- Additional MathFunc entries as GLSL coverage expands.
- **Vector types and ops** — additive extension for SIMD backends (WASM
  v128, ESP32-P4 PIE). See overview chapter.
- **64-bit types** — `i64`, `f64` if needed in the future.
- **Diagnostic / safe mode** — a validation pass or interpreter flag that
  warns on numeric edge cases (div-by-zero, NaN inputs, out-of-range casts
  before saturation, out-of-bounds memory). Never changes results — only
  reports. Helps shader developers catch undefined-behavior-dependent code.

## Validate

Review the section for:
- Grammar is complete and covers all ops, control flow, and declarations.
- Grammar is LL(1)-parseable (no ambiguity in lookahead).
- Lexical rules are explicit (no implicit assumptions).
- Every op appears in at least one example.
- Examples are syntactically valid according to the grammar.
- The future extensions section lists all known-needed ops.
