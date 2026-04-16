# Debug Output Unification - Analysis Notes

## Scope of Work

Unify debug output across the compiler to use a single, consistent format based on the filetest interleaved style. This involves:

1. Creating a standardized `ModuleDebugInfo` / `FunctionDebugInfo` system in `lpvm`
2. Using the interleaved format (as seen in `.lpir` filetests) as the canonical debug output format
3. Creating a unified `shader-debug` CLI command that clearly shows sections
4. Removing the many `--no-*` flags from current commands
5. Ensuring copy-pasteable help text for discoverability

## Current State

### FA Backend (`lpvm-native`)

**Two different debug output formats:**

1. **`render_alloc_output`** (used in `shader-rv32fa --pinst`):
   - Raw VInst sequence with allocations
   - Section markers like `=== Alloc snapshot ===`
   - Not interleaved with LPIR

2. **`render_interleaved`** (used in `.lpir` filetests):
   - LPIR operations interleaved with VInsts and allocations
   - Shows the mapping from high-level LPIR to low-level VInst
   - Includes move/read/write annotations and traces
   - Example from `arg_reuse.lpir`:
     ```
     v3:i32 = call @filetest::f(v1, v2)
         ; move: a1 -> s2
         ; move: a2 -> t5
         i3 = Call f (i1, i2)
         ; write: i3 -> a0
         ; trace: call_ret: x10 -> x31 (v3)
     ```

**Existing debug infrastructure:**
- `NativeEmuModule` has `debug_asm: BTreeMap<String, String>` field
- Populated during compilation in `compile_function`
- Contains VInst section and Disasm section
- NOT the interleaved format

### Cranelift Backend (`lpvm-native` / `lpvm-emu`)

**Debug output:**
- `compile_module_asm_text` produces disassembly with LPIR interleaving
- Uses `disassemble_function` which includes hex offsets and LPIR comments
- No allocation-level debug info (Cranelift handles that internally)

### CLI Commands

**Current `shader-rv32fa`:**
- Many flags: `--no-lpir`, `--no-vinst`, `--no-pinst`, `--no-disasm`, `--show-region`, `--show-liveness`, `--quiet`
- Outputs to stderr vs stdout inconsistently
- Different formats for each section

**Proposed `shader-debug`:**
- Single `--target` flag (e.g., `-t rv32fa` or `-t rv32`)
- No granular flags - always shows all sections clearly labeled
- Consistent format across all backends

## Questions & Answers

### Q1: Interleaved format for Cranelift

**Question:** The interleaved format is FA-specific. Should we show it for Cranelift too?

**Answer:** Option C - show interleaved where available, fall back to disasm with a note. The Cranelift backend doesn't have VInst-level visibility, so it only shows disassembly.

### Q2: CLI approach

**Question:** Create new command or simplify existing ones?

**Answer:** Option A - create new `shader-debug` command and **completely remove** `shader-rv32fa` and `shader-rv32`.

### Q3: Function filtering

**Question:** Show all functions by default or require `--fn`?

**Answer:** Option A - show all by default, allow `--fn` to filter. Include copy-pasteable help text at the end showing how to narrow down.

### Q4: Feature gating

**Question:** Feature-gate the debug system?

**Answer:** Option C - Backend-specific. The backends that support debug info populate it, others return None. No extra cargo features needed.

### Q5: Discoverability

**Question:** How to make the command easy to discover and use?

**Answer:** Include help text at the end of every output with copy-pasteable commands like:
```
────────────────────────────────────────
To show a specific function:
  lp-cli shader-debug -t rv32fa file.glsl --fn test_foo

Available functions: callee_identity, test_no_preserve_across_call, ...
```

## Implementation Phases

1. **Core Types** - Create `ModuleDebugInfo` and `FunctionDebugInfo` in `lpvm`
2. **FA Backend** - Refactor to populate structured debug info
3. **Cranelift Backends** - Populate disassembly sections
4. **CLI Command** - Create `shader-debug` and remove old commands
5. **Cleanup** - Remove old code, fix warnings, validate

See phase files for detailed implementation instructions.
