# Phase 3: Implement VCode and Assembly Extraction

## Description

Rename `clif.rs` to `codegen.rs` and add functions to compile functions, extract vcode and assembly from compiled code, and write vcode/assembly files. Follow the pattern from `build_emu_executable` in the emulator codegen.

## Implementation

- Rename `src/clif.rs` to `src/codegen.rs`
- Update module declaration in `src/main.rs` from `mod clif;` to `mod codegen;`
- Add `extract_compiled_code()` function:
  - Takes context and module
  - Accesses `ctx.compiled_code()`
  - Extracts vcode from `compiled_code.vcode`
  - Generates disassembly using Capstone (with fallback to vcode)
  - Returns `(Option<String>, Option<String>)` for (vcode, assembly)
- Add `compile_function_and_extract()` function:
  - Takes module, function name, function IR, and func_id
  - Creates context and sets function
  - Calls `ctx.set_disasm(true)` to enable disassembly
  - Calls `define_function()` to compile
  - Calls `extract_compiled_code()` to get vcode/assembly
  - Clears context
  - Returns vcode and assembly strings
- Update `write_clif_files()` to `write_codegen_files()`:
  - Keep existing CLIF writing logic
  - For each function, compile both before and after versions
  - Extract vcode and assembly for both
  - Write `.pre.vcode`, `.post.vcode`, `.pre.s`, `.post.s` files
  - Return `HashMap<String, (usize, usize)>` mapping function names to (vcode_size, assembly_size)
- Update `process_test()` in `main.rs`:
  - Change `clif::write_clif_files()` to `codegen::write_codegen_files()`
  - Store returned vcode/assembly sizes for use in statistics

## Success Criteria

- VCode files are written for all functions (before and after)
- Assembly files are written for all functions (before and after)
- File names follow pattern: `<function-name>.pre.vcode`, `<function-name>.post.vcode`, `<function-name>.pre.s`, `<function-name>.post.s`
- Function returns vcode/assembly sizes for statistics
- Code compiles without errors
- No warnings (except unused code that will be used in later phases)

## Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Use measured, factual descriptions of what was implemented
