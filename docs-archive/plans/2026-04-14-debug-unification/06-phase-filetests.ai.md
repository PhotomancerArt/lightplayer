# Phase 6: Filetest Debug Integration

## Scope

Update the filetest system to use `ModuleDebugInfo` for detail/debug output. This ensures running filetests with `--debug` shows the same unified format as `shader-debug`.

## Implementation Details

### 1. Update `lps-filetests/src/test_run/filetest_lpvm.rs`

The `CompiledShader` enum wraps all backend modules. Add a method to extract debug info:

```rust
impl CompiledShader {
    /// Get debug info from the compiled module if available.
    pub fn debug_info(&self) -> Option<&ModuleDebugInfo> {
        use lpvm::LpvmModule;
        match self {
            Self::Jit(_) => None,  // JIT doesn't support debug
            Self::Emu(m) => m.debug_info(),
            Self::Native(m) => m.debug_info(),
            Self::NativeFa(m) => m.debug_info(),
            Self::Wasm(_) => None,  // WASM doesn't support debug
        }
    }
}
```

### 2. Update `lps-filetests/src/test_run/run_detail.rs`

In the detail mode output, use `ModuleDebugInfo` instead of manual formatting:

```rust
// After successful compilation, print debug info if available
if output_mode.show_debug_sections() {
    if let Some(debug_info) = compiled.debug_info() {
        let rendered = debug_info.render(None);  // All functions
        eprintln!("\n=== Debug Info ===\n{}", rendered);
        eprintln!("{}", debug_info.help_text(&relative_path, target.name()));
    } else {
        eprintln!("\n(Debug info not available for {})", target.name());
    }
}
```

### 3. Ensure Consistent Output

The filetest detail output and `shader-debug` command should produce identical debug sections. Verify:

- Section ordering (interleaved, disasm, vinst, liveness, region)
- Header format ("=== Function: name ===", "--- section ---")
- Help text format with copy-pasteable commands

### 4. Add `--fn` Support to Filetests (Optional Enhancement)

If we want to match `shader-debug` exactly, add function filtering to filetests:

```rust
// In TestOptions (main.rs)
#[arg(long)]
fn_filter: Option<String>,

// In run_detail.rs, pass to render()
let rendered = if let Some(filter) = fn_filter {
    debug_info.render(Some(filter))
} else {
    debug_info.render(None)
};
```

This would enable:

```bash
scripts/filetests.sh --target rv32fa.q32 --fn test_foo file.glsl
```

## Usage Examples

After this phase, both commands show the same unified format:

### shader-debug

```bash
lp-cli shader-debug -t rv32fa file.glsl
```

### filetests (with debug output)

```bash
# Detail mode shows debug info
scripts/filetests.sh --target rv32fa.q32 file.glsl

# Debug mode (same sections as shader-debug)
DEBUG=1 scripts/filetests.sh --target rv32fa.q32 file.glsl
```

Both show:

```
=== Function: test_foo ===

--- interleaved (7 VInsts) ---
func @test_foo(v1:i32) -> i32 {
    ...
}

--- disasm (7 instructions) ---
...

────────────────────────────────────────
To show a specific function:
  lp-cli shader-debug -t rv32fa file.glsl --fn test_foo
```

## Tests

Run filetests with debug output to verify:

```bash
# Single file, detail mode
scripts/filetests.sh --target rv32fa.q32 lpvm/native/perf/caller-save-pressure.glsl

# Check that debug sections appear
scripts/filetests.sh --target rv32fa.q32,rv32.q32 lpvm/native/perf/caller-save-pressure.glsl 2>&1 | grep -A5 "=== Function:"
```

## Code Organization

- `filetest_lpvm.rs` - Add `debug_info()` method to `CompiledShader`
- `run_detail.rs` - Use `ModuleDebugInfo::render()` for debug output
- Keep changes minimal - just wire up the existing trait method

## Relationship to Other Phases

This phase depends on:

- Phase 2 (FA backend populating debug info)
- Phase 3 (Cranelift backends populating debug info)
- Phase 4 (CLI command using debug info)

It can be done in parallel with Phase 4-5, or after.
