# Phase 2: FA Backend ModuleDebugInfo Population

## Scope

Refactor `lpvm-native-fa` to populate `ModuleDebugInfo` instead of the raw `debug_asm: BTreeMap<String, String>`. Use the interleaved format as the primary output.

## Implementation Details

### 1. Update `lpvm-native-fa/src/rt_emu/module.rs`

Change the field type:
```rust
pub struct NativeEmuModule {
    pub(crate) ir: LpirModule,
    pub(crate) _elf: Vec<u8>,
    pub(crate) meta: LpsModuleSig,
    pub(crate) load: Arc<lp_riscv_elf::ElfLoadInfo>,
    pub(crate) arena: EmuSharedArena,
    pub(crate) options: NativeCompileOptions,
    /// Debug info (replaces debug_asm field).
    pub(crate) debug_info: ModuleDebugInfo,
}

impl LpvmModule for NativeEmuModule {
    // ... existing methods ...

    fn debug_info(&self) -> Option<&ModuleDebugInfo> {
        Some(&self.debug_info)
    }
}
```

### 2. Update `lpvm-native-fa/src/rt_emu/engine.rs`

In the compile method, populate `ModuleDebugInfo`:
```rust
// After compiling each function:
let mut debug_info = ModuleDebugInfo::new();

for func in &ir.functions {
    let lowered = lower_ops(func, ir, &module_abi, float_mode)?;
    let func_abi = ...;
    let alloc_result = fa_alloc::allocate(&lowered, &func_abi)?;
    let emitted = emit_lowered_with_alloc(...)?;

    // Build interleaved section
    let interleaved = render_interleaved(
        func, ir, &lowered.vinsts, &lowered.vreg_pool,
        &alloc_result.output, &func_abi, &lowered.symbols
    );

    // Build disasm section
    let mut disasm = String::new();
    let mut off = 0usize;
    while off + 4 <= emitted.code.len() {
        let w = u32::from_le_bytes(...);
        disasm.push_str(&format!("{:04x}\t{:08x}\t{}\n", ...));
        off += 4;
    }

    // Build optional sections if options.enable_extra_debug
    let mut sections = BTreeMap::new();
    sections.insert("interleaved".into(), interleaved);
    sections.insert("disasm".into(), disasm);

    if options.enable_vinst {
        let mut vinst_text = String::new();
        for inst in &lowered.vinsts {
            vinst_text.push_str(&format!("{} ...\n", ...));
        }
        sections.insert("vinst".into(), vinst_text);
    }

    let func_debug = FunctionDebugInfo::new(&func.name)
        .with_inst_count(emitted.code.len() / 4)
        .with_sections(sections); // or use builder pattern

    debug_info.add_function(func_debug);
}
```

### 3. Update `lpvm-native-fa/src/compile.rs`

The `CompiledFunction` struct should also carry `FunctionDebugInfo`:
```rust
pub struct CompiledFunction {
    pub name: String,
    pub code: Vec<u8>,
    pub relocs: Vec<NativeReloc>,
    pub debug_lines: Vec<(u32, Option<u32>)>,
    pub debug_info: FunctionDebugInfo,  // NEW: replaces debug_asm
}
```

Update `compile_function` to return the full `FunctionDebugInfo` instead of just the `debug_asm` string.

### 4. Add helper to `fa_alloc/render.rs`

Add a method to render to a sections map:
```rust
/// Render AllocOutput to a sections map for debug info.
pub fn render_to_sections(
    func: &IrFunction,
    module: &LpirModule,
    lowered: &LoweredFunction,
    output: &AllocOutput,
    func_abi: &FuncAbi,
) -> BTreeMap<String, String> {
    let mut sections = BTreeMap::new();
    
    // interleaved
    sections.insert(
        "interleaved".into(),
        render_interleaved(func, module, &lowered.vinsts, &lowered.vreg_pool, output, func_abi, &lowered.symbols)
    );
    
    // liveness (optional)
    if env::var("LPVM_DEBUG_LIVENESS").is_ok() {
        let liveness = analyze_liveness(...);
        sections.insert("liveness".into(), format_liveness(&liveness));
    }
    
    // region (optional)
    if env::var("LPVM_DEBUG_REGION").is_ok() {
        sections.insert("region".into(), format_region_tree(...));
    }
    
    sections
}
```

### 5. Handle rt_jit module

Update `lpvm-native-fa/src/rt_jit/module.rs` similarly:
```rust
pub struct NativeJitModule {
    pub inner: Arc<NativeJitModuleInner>,
    pub debug_info: ModuleDebugInfo,  // NEW
}

impl LpvmModule for NativeJitModule {
    fn debug_info(&self) -> Option<&ModuleDebugInfo> {
        Some(&self.debug_info)
    }
}
```

## Code Organization

- Keep compilation logic in `compile.rs`
- Keep module struct definitions in `rt_emu/module.rs` and `rt_jit/module.rs`
- The `render_to_sections` helper goes in `fa_alloc/render.rs` near other render functions
- Use environment variables for optional sections (liveness, region) to avoid adding more compile options

## Tests

Run existing FA tests to ensure debug info is still populated:
```bash
cargo test -p lpvm-native-fa --lib
```

Verify the `debug_info` method returns valid data:
```rust
#[test]
fn fa_module_has_debug_info() {
    // Compile a simple function
    let module = compile_test_function();
    let debug = module.debug_info().expect("should have debug info");
    assert!(debug.functions.contains_key("test"));
    let func = &debug.functions["test"];
    assert!(func.sections.contains_key("interleaved"));
    assert!(func.sections.contains_key("disasm"));
    assert!(func.inst_count > 0);
}
```

## Validate

```bash
cargo check -p lpvm-native-fa
cargo test -p lpvm-native-fa
```
