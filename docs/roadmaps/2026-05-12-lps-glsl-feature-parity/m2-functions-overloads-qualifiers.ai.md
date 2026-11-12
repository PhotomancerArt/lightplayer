# M2: Functions, Overloads, and Parameter Qualifiers

## Objective

Support the function surface used by real shaders and filetests, including overload resolution and `out`/`inout` behavior.

## Features

- function prototypes and forward references if the corpus uses them
- overload sets by name and parameter types
- `void` functions
- early returns from nested control flow
- parameter qualifiers: `in`, `out`, `inout`
- copy-in/copy-out call semantics
- lvalue checking for `out` and `inout` arguments
- nested user calls
- richer constructor and cast resolution

## Implementation Notes

Keep overload resolution in semantic analysis, not in the parser. The syntax layer should preserve enough declared type and qualifier information to let the semantic layer decide.

`out` and `inout` should reuse the M1 lvalue path machinery. This avoids inventing a second writeback mechanism just for calls.

Prefer a small call-resolution module over distributing overload logic across parser, HIR, and lowering.

Useful internal concepts:

- `FunctionSignature`
- `ParamQualifier`
- `ResolvedCall`
- `CallWriteback`

## Filetest Gate

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise function lpfn
```

Also keep the M1 gate running:

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl scalar vec operators control
```

## Done

- ordinary and overloaded functions compile and execute
- `out` and `inout` work for simple locals and legal projected lvalues
- invalid `out`/`inout` call sites get useful diagnostics
- call lowering stays compatible with resumable compilation

