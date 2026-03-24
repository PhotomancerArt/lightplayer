# Text Format

This chapter defines the lexical structure and a line-oriented, semi-formal grammar for LPIR text. The grammar is written so that a recursive-descent parser can disambiguate alternatives with single-token lookahead under the usual keyword and identifier rules.

## Lexical rules

- **Whitespace:** Space (U+0020) and tab (U+0009) separate tokens. Whitespace has no other meaning.
- **Line-oriented statements:** Each non-block statement occupies a single line. Block constructs (`if`, `else`, `loop`, `switch`, `case`, `default`, function bodies) span multiple lines; their headers and opening braces begin lines as shown in the grammar.
- **Comments:** `;` starts a line comment; the comment runs to the end of the line. Comments behave like whitespace for tokenization.
- **Identifiers:** `[a-zA-Z_][a-zA-Z0-9_.]*`
- **VReg names:** `v` followed by one or more decimal digits (`v0`, `v12`, …).
- **Slot names:** `ss` followed by one or more decimal digits (`ss0`, …).
- **Local function names:** `@` followed by an identifier (`@main`, `@helper`).
- **Import-qualified names:** `@` identifier `::` identifier (`@std.math::fsin`).
- **Integer literals:** Optional leading `-` for decimal; decimal digit sequences; or `0x` / `0X` followed by hexadecimal digits. The grammar uses `integer_literal` for signed-style integers where the operation defines interpretation (for example immediate operands).
- **Unsigned offset / size literals:** Non-negative integer literals used for `load` / `store` offsets, `memcpy` sizes, and slot sizes. The grammar uses `uint_literal`.
- **Float literals:** Decimal floating-point with a fractional part (`1.5`, `-0.0`), or the keywords `inf`, `-inf`, `nan` as accepted by the concrete parser for `fconst.f32`.
- **Keywords:** `func`, `entry`, `import`, `slot`, `if`, `else`, `loop`, `break`, `continue`, `return`, `br_if_not`, `switch`, `case`, `default`, `call`, `select`, `copy`, `load`, `store`, `slot_addr`, `memcpy`, `f32`, `i32`, and every opcode spelling used in the `op` productions below. An identifier that matches a keyword is a keyword, not a generic identifier.

## Immediate variants

The following opcodes take a literal immediate as the final operand instead of a second VReg:

- `iadd_imm`
- `isub_imm`
- `imul_imm`
- `ishl_imm`
- `ishr_s_imm`
- `ishr_u_imm`
- `ieq_imm`

## Grammar (EBNF)

Metasymbols: `{ … }` repetition, `[ … ]` optional, `|` alternative, `( … )` grouping. Tokens `"{` and `"}"` are literal braces. `EOL` denotes end of line (after lexical comment stripping, a statement line ends at EOL).

```
module           = { module_item }
module_item      = import_decl | func_decl | entry_func_decl

import_decl      = "import" import_name "(" type_list ")" [ "->" return_type ] EOL
import_name      = "@" identifier "::" identifier

func_decl        = "func" local_func_name "(" param_list ")" [ "->" return_type ] "{" func_body "}"
entry_func_decl  = "entry" "func" local_func_name "(" param_list ")" [ "->" return_type ] "{" func_body "}"

local_func_name  = "@" identifier
param_list       = [ vreg_def { "," vreg_def } ]
type_list        = [ type { "," type } ]

return_type      = type | "(" type { "," type } ")"
type             = "f32" | "i32"

func_body        = { slot_line } { inner_line }
slot_line        = "slot" slot_name "," uint_literal EOL

inner_line       = statement EOL
statement        = assign_stmt | void_stmt | if_stmt | loop_stmt
                 | switch_stmt | return_stmt | "break" | "continue" | br_if_not_stmt

assign_stmt      = vreg_list "=" rhs
void_stmt        = store_stmt | memcpy_stmt | void_call_stmt

vreg_list        = vreg_def { "," vreg_def }
vreg_def         = vreg [ ":" type ]
vreg             = "v" dec_digits
slot_name        = "ss" dec_digits
dec_digits       = digit { digit }
digit            = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"

identifier       = (* lexical: [a-zA-Z_][a-zA-Z0-9_.]* *)
uint_literal     = dec_digits | hex_literal
hex_literal      = ( "0x" | "0X" ) hex_digit { hex_digit }
hex_digit        = digit | "a"|"b"|"c"|"d"|"e"|"f"|"A"|"B"|"C"|"D"|"E"|"F"
integer_literal  = [ "-" ] uint_literal
float_literal    = (* lexical: decimal float or inf / -inf / nan; see Lexical rules *)

rhs              = op | call_expr
call_expr        = "call" callee "(" [ vreg { "," vreg } ] ")"
void_call_stmt   = call_expr
callee           = import_name | local_func_name

store_stmt       = "store" vreg "," uint_literal "," vreg
memcpy_stmt      = "memcpy" vreg "," vreg "," uint_literal

if_stmt          = "if" vreg "{" { inner_line } "}" [ "else" "{" { inner_line } "}" ]
loop_stmt        = "loop" "{" { inner_line } "}"

switch_stmt      = "switch" vreg "{" { switch_case } [ default_case ] "}"
switch_case      = "case" integer_literal "{" { inner_line } "}"
default_case     = "default" "{" { inner_line } "}"

return_stmt      = "return" [ vreg { "," vreg } ]
br_if_not_stmt   = "br_if_not" vreg

op               = const_op | unary_op | binary_op | imm_op | select_op | copy_op
                 | slot_addr_op | load_op

const_op         = fconst_op | iconst_op
fconst_op        = "fconst.f32" float_literal
iconst_op        = "iconst.i32" integer_literal

unary_op         = unary_opcode vreg
unary_opcode     = "fneg" | "ineg" | "ibnot"
                 | "ftoi_sat_s" | "ftoi_sat_u" | "itof_s" | "itof_u"

binary_op        = binary_opcode vreg "," vreg
binary_opcode    = "fadd" | "fsub" | "fmul" | "fdiv"
                 | "iadd" | "isub" | "imul" | "idiv_s" | "idiv_u" | "irem_s" | "irem_u"
                 | "feq" | "fne" | "flt" | "fle" | "fgt" | "fge"
                 | "ieq" | "ine" | "ilt_s" | "ile_s" | "igt_s" | "ige_s"
                 | "ilt_u" | "ile_u" | "igt_u" | "ige_u"
                 | "iand" | "ior" | "ixor" | "ishl" | "ishr_s" | "ishr_u"

imm_op           = imm_opcode vreg "," integer_literal
imm_opcode       = "iadd_imm" | "isub_imm" | "imul_imm"
                 | "ishl_imm" | "ishr_s_imm" | "ishr_u_imm" | "ieq_imm"

select_op        = "select" vreg "," vreg "," vreg
copy_op          = "copy" vreg

slot_addr_op     = "slot_addr" slot_name
load_op          = "load" vreg "," uint_literal
```

Notes on the grammar:

- `float_literal` and the precise accepted spellings for infinities and NaNs are defined by the parser implementation; the lexical rules above list the intended surface forms.
- `assign_stmt` covers multi-result `call` when `rhs` is `call_expr` and `vreg_list` has more than one `vreg_def`; arity must match the callee’s declared return type list.
- `op` appears only as the right-hand side of `assign_stmt` (including `v0 = op` reassignments where `vreg_def` omits `: type`).
- `identifier` in `local_func_name` and in `import_name` uses the lexical identifier rule.

## Well-formedness (module)

- Every `call` callee is declared in the same module as an `import` or as a `func` / `entry func`.
- There is at most one `entry func`.
- Function names are unique: local names use `@identifier`; imports use `@module::name`. No two declarations share the same callee spelling.
- Every call site’s argument and result types are consistent with the callee’s declared signature.
- Call graphs may be cyclic (recursion is allowed). Stack overflow is implementation-defined termination.

## Well-formedness (function)

- Every VReg used on a line is defined earlier in the function (by parameter list, or by a prior defining assignment on some control-flow path). Parameters in `param_list` are defined at entry with the types given there.
- Each VReg has a single concrete type for its entire lifetime; redefinitions use the same type as the parameter or first definition.
- Each `op` and `call` obeys the operand and result typing rules in `02-core-ops.md`, `03-memory.md`, and `05-calls.md` (including `select` branch types matching the result type).
- `br_if_not` appears only inside a `loop` body.
- `break` and `continue` appear only inside a `loop` body.
- Every `slot_addr` references a `slot` declared in the same function; the slot’s size is the declared `uint_literal` for that slot.
- In each `switch`, case values are compile-time integer constants, pairwise distinct, and there is at most one `default`.
- `return` matches the enclosing function’s return arity: no VReg for void; one VReg for a single scalar return; a parenthesized return type in the declaration corresponds to multiple parallel returned scalars as defined by the calls chapter.
- Braces for `if` / `else` / `loop` / `switch` / `case` / `default` / function bodies are properly nested and closed; there is no implicit fall-through between `switch` cases.
