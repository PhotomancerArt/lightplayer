# LPIR Stage II — Summary

## Implemented

The `lp-shader/lpir` crate (`#![no_std]` + `alloc`) provides:

- **Types** (`types.rs`): `IrType`, `VReg`, `SlotId`, `VRegRange`, `CalleeRef`
- **Ops** (`op.rs`): flat `Op` enum with control-flow markers, skip offsets, and `Call`/`Return`
  using `VRegRange` into `vreg_pool`
- **Module** (`module.rs`): `IrModule`, `IrFunction`, `ImportDecl`, `SlotDecl`
- **Builder** (`builder.rs`): `FunctionBuilder`, `ModuleBuilder` with stack-based control flow and
  offset patching
- **Printer** (`print.rs`): LPIR text output aligned with `docs/lpir/07-text-format.md`
- **Parser** (`parse.rs`): hand-rolled scanner (no `nom` dependency); parses the spec grammar used
  by tests
- **Interpreter** (`interp.rs`): `interpret` / `interpret_with_depth`, `Value`, `ImportHandler`,
  `InterpError` (`Display` + `Error`)
- **Validator** (`validate.rs`): module and function checks including vreg use, control-flow
  nesting, calls, returns, slots, pool bounds, and opcode vs `vreg_types` consistency for defining
  ops

Public re-exports are in `lib.rs` (`parse_module`, `print_module`, interpreter types, validation).

## Design choices (from plan)

- Single flat `Vec<Op>` per function; generic `End`; markers carry `u32` offsets
- `vreg_pool` for variable-arity `Call` / `Return`
- Parser: stage-II plan mentioned `nom` + `nom_locate`; the shipped parser is hand-rolled to avoid
  extra dependencies while covering the same surface (including ops exercised by
  `round_trip_all_ops`)

## Tests

- Round-trip tests for spec-style examples (control flow, memory, imports, constants, multi-return,
  entry)
- `round_trip_all_ops`: builder-built module covering every `Op` variant → print → parse → print
- Parse error smoke tests
- Validation positive/negative tests (vregs, control flow, imports, entry, calls, returns, slots,
  pool, copy/select types, duplicate switch case, duplicate function names)
- `interp_add` and `InterpError` display
- `Op` size bound: `size_of::<Op>() <= 32` on current targets (initial ~20-byte target was not met
  on 64-bit; test documents that)

## Deferred (Stage III / later)

- `nom` / `nom_locate` if stricter span diagnostics or composable grammar maintenance is desired
- Path-sensitive “defined on some path” vreg analysis (validator uses linear scan as allowed by
  plan)
- Full offset-target validation for every marker (`Else`/`End` pairing at referred indices)
