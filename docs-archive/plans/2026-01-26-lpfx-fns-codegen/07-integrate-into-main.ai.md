# Phase 7: Integrate into main codegen flow

## Description

Integrate LPFX codegen into the main `lps-builtin-gen-app` flow and update file generation.

## Implementation

1. Update `lps-builtin-gen-app/src/main.rs`:
    - Add call to discover LPFX functions
    - Add call to validate LPFX functions
    - Add call to generate `lpfn_fns.rs`
    - Add generated file to formatting list
2. Set output path: `lp-shader/lps-compiler/src/frontend/semantic/lpfn/lpfn_fns.rs`
3. Ensure generated file is formatted with `cargo fmt`

## Success Criteria

- Codegen runs as part of main flow
- Generates `lpfn_fns.rs` in correct location
- Generated file is formatted
- Code compiles
