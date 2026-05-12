# Debug Output Unification Design

## Scope of Work

1. Create a standardized `ModuleDebugInfo` / `FunctionDebugInfo` trait system in `lpvm`
2. Unify all compiler debug output to use the interleaved format (LPIR + VInst + allocations) as the canonical format
3. Create a new `shader-debug` CLI command with clear section-based output
4. Remove `shader-rv32fa` and `shader-rv32` CLI commands entirely
5. Ensure copy-pasteable help text for discoverability

## File Structure

```
lp-shader/lpvm/src/
├── debug.rs                          # NEW: ModuleDebugInfo, FunctionDebugInfo types
└── module.rs                         # UPDATE: Add debug_info() trait method

lp-shader/lpvm-native/src/
├── rt_emu/module.rs                  # UPDATE: Change debug_asm field to ModuleDebugInfo
├── rt_emu/engine.rs                  # UPDATE: Populate ModuleDebugInfo during compile
├── compile.rs                        # UPDATE: Return FunctionDebugInfo per function
└── fa_alloc/
    └── render.rs                     # UPDATE: Add render_to_sections() for debug info

lp-shader/lpvm-native/src/
├── rt_emu/module.rs                  # UPDATE: Add ModuleDebugInfo field
├── rt_emu/engine.rs                  # UPDATE: Populate with disasm section
└── debug_asm.rs                      # UPDATE: Return per-function disasm

lp-shader/lpvm-emu/src/
├── module.rs                         # UPDATE: Add ModuleDebugInfo with disasm
└── engine.rs                         # UPDATE: Populate during compile

lp-cli/src/commands/
├── mod.rs                            # UPDATE: Remove shader_rv32, shader_rv32fa modules
├── shader_debug/                     # NEW: Unified debug command
│   ├── mod.rs
│   ├── args.rs
│   └── handler.rs
└── (delete shader_rv32/ and shader_rv32fa/)

lp-shader/lps-filetests/src/
└── test_run/
    ├── detail.rs                     # UPDATE: Use ModuleDebugInfo for detail mode
    └── filetest_lpvm.rs              # UPDATE: Add debug_info() method to CompiledShader
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     shader-debug CLI                            │
│  lp-cli shader-debug -t rv32fa file.glsl [--fn test_foo]       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                   Compilation Pipeline                           │
│  GLSL → LPIR → (Backend-specific lowering) → Machine Code      │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │  Cranelift   │  │  FA (native)  │  │  JIT/WASM    │          │
│  │  (rv32)      │  │  (rv32fa)    │  │  (no debug)  │          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
│         │                  │                                     │
│         ▼                  ▼                                     │
│  ┌──────────────┐  ┌──────────────┐                              │
│  │ disasm only  │  │ interleaved  │                              │
│  │ section      │  │ + disasm     │                              │
│  └──────────────┘  └──────────────┘                              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                  ModuleDebugInfo Output                          │
│                                                                  │
│  Function: test_foo                                              │
│  ┌────────────────────────────────────────────────────────┐     │
│  │ interleaved                                            │     │
│  │   (LPIR + VInst + allocations + traces)                │     │
│  └────────────────────────────────────────────────────────┘     │
│  ┌────────────────────────────────────────────────────────┐     │
│  │ disasm (7 instructions)                                │     │
│  │   0000  addi sp, sp, -16                               │     │
│  │   ...                                                  │     │
│  └────────────────────────────────────────────────────────┘     │
│                                                                  │
│  [Help text with copy-pasteable commands]                        │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                  Filetest Detail Mode                            │
│                                                                  │
│  scripts/filetests.sh --target rv32fa.q32 file.glsl        │
│                                                                  │
│  Uses same ModuleDebugInfo::render() output                     │
│  Consistent format between shader-debug and filetests           │
└─────────────────────────────────────────────────────────────────┘
```

## Main Components

### 1. Core Types (`lpvm/src/debug.rs`)

```rust
/// Per-function compilation debug info.
pub struct FunctionDebugInfo {
    pub name: String,
    pub inst_count: usize,
    /// Named sections: "interleaved", "disasm", "vinst", "liveness", "region"
    pub sections: BTreeMap<String, String>,
}

/// Module-level compilation debug info.
pub struct ModuleDebugInfo {
    pub functions: BTreeMap<String, FunctionDebugInfo>,
}

impl ModuleDebugInfo {
    /// Render all functions to a single string with clear section headers.
    pub fn render(&self, fn_filter: Option<&str>) -> String;

    /// Get help text with copy-pasteable commands.
    pub fn help_text(&self, file_path: &str, target: &str) -> String;
}
```

### 2. Trait Extension (`lpvm/src/module.rs`)

```rust
pub trait LpvmModule {
    // ... existing methods ...

    /// Compilation debug info. Returns None if not available for this backend.
    fn debug_info(&self) -> Option<&ModuleDebugInfo> {
        None
    }
}
```

### 3. FA Backend Population (`lpvm-native`)

The existing `compile_function` already produces debug info. Refactor to:

1. Generate `interleaved` section using `render_interleaved()`
2. Generate `disasm` section from emitted code
3. Generate optional `liveness` and `region` sections if requested
4. Store in `FunctionDebugInfo` instead of raw `debug_asm` string

### 4. Cranelift Backend Population (`lpvm-native`, `lpvm-emu`)

Use existing `compile_module_asm_text` / `disassemble_function` output:

1. Split per-function (already done by `.globl` headers)
2. Store as `disasm` section only
3. No interleaved section (backend doesn't have VInst visibility)

### 5. CLI Command (`lp-cli shader-debug`)

**Arguments:**

- `--target, -t <backend>` (required): e.g., `rv32fa`, `rv32`, `rv32lp`
- `--fn <name>` (optional): Filter to single function
- `--float-mode` (optional): `q32` (default) or `f32`
- `input` (required): Path to `.glsl` file

**Behavior:**

1. Parse GLSL, lower to LPIR
2. Compile with specified backend
3. Get `debug_info()` from module
4. Render all sections for all functions (or filtered function)
5. Print help text at end with copy-pasteable examples

**Output Format:**

```
=== Function: callee_identity ===

--- interleaved (7 VInsts) ---
func @callee_identity(v1:i32) -> i32 {
    ; spill_slots: 0
    ; arg v1: a1
    ; ret: a0

    v1 = copy v1
        ; move: a1 -> t4
        ; read: i1 <- t4
        Mov32 (unformatted)
        ; write: i2 -> t4
        ; trace: coalesce: v1 -> t29 (shared)

    return v2
        ; read: i2 <- t4
        Ret i2
        ; trace: alloc: v2 -> t29
}

--- disasm (7 instructions) ---
.globl	callee_identity
	ff010113	addi sp, sp, -16
	00812623	sw s0, 12(sp)
	...

=== Function: test_no_preserve_across_call ===
...

────────────────────────────────────────
To show a specific function:
  lp-cli shader-debug -t rv32fa lpvm/native/perf/caller-save-pressure.glsl --fn callee_identity

Available functions: callee_identity, test_no_preserve_across_call, ...
```

## Implementation Notes

### Backward Compatibility

The `shader-rv32fa` and `shader-rv32` commands will be **removed**. Users who need binary/text output should use:

- Filetests for testing
- Direct crate APIs for programmatic use

### Section Availability by Backend

| Backend | interleaved | disasm | vinst | liveness | region |
| ------- | ----------- | ------ | ----- | -------- | ------ |
| rv32fa  | ✓           | ✓      | ✓     | ✓        | ✓      |
| rv32lp  | ✗           | ✓      | ✗     | ✗        | ✗      |
| rv32    | ✗           | ✓      | ✗     | ✗        | ✗      |
| jit     | ✗           | ✗      | ✗     | ✗        | ✗      |
| wasm    | ✗           | ✗      | ✗     | ✗        | ✗      |

Missing sections render as:

```
--- interleaved ---
(not available for rv32 backend - only disassembly available)
```

### Rendering Strategy

The `ModuleDebugInfo::render()` method:

1. Iterates functions in module order
2. For each function, prints name header
3. For each section, prints subsection header with count
4. Includes copy-pasteable help at end

No options to hide sections - always show everything. The philosophy is "make it easy to see what you need, not to hide what you don't".
