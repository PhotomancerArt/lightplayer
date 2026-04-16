# Phase 8: Cleanup & validation

## Scope

Final cleanup: remove TODOs from earlier phases, fix warnings, verify all tests
pass, ensure no dead code remains. Commit.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Grep for TODOs

```bash
rg 'TODO' lp-shader/lpvm-native/src/ --type rust
```

Resolve each TODO:
- `compile.rs`: "TODO: wire up debug_lines" — either wire it or leave with a
  clear comment that debug lines require the full EmitContext path (M4+ work)
- Any stubs from phase 2 should be gone by now

### 2. Grep for dead code

```bash
rg 'allow\(dead_code\)' lp-shader/lpvm-native/src/ --type rust
```

Remove any dead code or `#[allow(dead_code)]` that was added during the
restructure.

### 3. Check for stale `isa` references

```bash
rg 'crate::isa' lp-shader/lpvm-native/src/ --type rust
rg 'isa::rv32' lp-shader/lpvm-native/src/ --type rust
```

Should return zero results.

### 4. Check for stale `regalloc` references

```bash
rg 'regalloc' lp-shader/lpvm-native/src/ --type rust
rg 'GreedyAlloc\|LinearScan\|RegAlloc\b' lp-shader/lpvm-native/src/ --type rust
```

Should return zero results.

### 5. Fix all warnings

```bash
cargo check -p lpvm-native 2>&1 | grep warning
cargo check -p lpvm-native --features emu 2>&1 | grep warning
```

Fix unused imports, dead code warnings, etc.

### 6. Format

```bash
cargo +nightly fmt -p lpvm-native
```

### 7. Run all tests

```bash
cargo test -p lpvm-native
cargo test -p lpvm-native --features emu
```

### 8. Check downstream

```bash
cargo check -p lp-engine
cargo check -p lp-cli
# If applicable:
# cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

## Plan cleanup

### Summary

Add a `summary.md` to the plan directory documenting:
- What was restructured
- The new file layout
- Key types introduced (CompileSession, CompiledFunction, CompiledModule)
- What was deleted

### Move plan files

```bash
mv docs/plans/2026-04-11-fastalloc-v2-m3-2-crate-restructure \
   docs/plans-done/2026-04-11-fastalloc-v2-m3-2-crate-restructure
```

## Commit

```
refactor(lpvm-native): restructure crate into compile/emit/link pipeline

- Flatten isa/rv32/ to rv32/ (one ISA, no trait indirection)
- Add compile.rs: CompileSession, compile_function, compile_module
- Add emit.rs: shared PInst → bytes emission
- Add link.rs: ELF generation (emu) and JIT relocation patching
- Simplify rt_jit and rt_emu to use compile + link
- Delete old 1470-line emit.rs monolith
- Delete dead IsaBackend trait, Rv32Backend, CodeBlob
- Delete regalloc/ (greedy, linear_scan) — lives in lpvm-native
- Module-level ModuleSymbols via CompileSession
```
