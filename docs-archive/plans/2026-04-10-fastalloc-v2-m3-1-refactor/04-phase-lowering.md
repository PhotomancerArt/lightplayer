# Phase 4: ModuleSymbols and LoweredModule

## Scope

Add module-level symbol interning and update the lowering infrastructure to produce:
- `ModuleSymbols` — interned callee names
- `LoweredModule` — top-level result containing functions + symbols
- `LoweredFunction` — extended with `vreg_pool` for Call/Ret operands

## Implementation

### 1. Create `ModuleSymbols` type

Add to `lower.rs` or new `module.rs`:

```rust
/// Module-level symbol table for interned callee names.
/// Shared across all functions in the module.
#[derive(Clone, Debug)]
pub struct ModuleSymbols {
    /// Interned symbol names. Index is SymbolId.
    pub names: Vec<String>,
    /// Reverse lookup: name -> SymbolId (for interning).
    /// Only used during lowering, cleared after to save memory.
    name_to_id: Option<alloc::collections::BTreeMap<String, SymbolId>>,
}

impl ModuleSymbols {
    /// Create empty symbol table.
    pub fn new() -> Self {
        Self {
            names: Vec::new(),
            name_to_id: Some(alloc::collections::BTreeMap::new()),
        }
    }

    /// Intern a symbol name, returning its SymbolId.
    /// If name already exists, returns existing id.
    pub fn intern(&mut self, name: &str) -> SymbolId {
        if let Some(id) = self.name_to_id.as_ref().and_then(|m| m.get(name).copied()) {
            return id;
        }
        
        let id = SymbolId(self.names.len() as u16);
        self.names.push(String::from(name));
        
        if let Some(ref mut m) = self.name_to_id {
            m.insert(String::from(name), id);
        }
        
        id
    }

    /// Look up symbol name by id.
    pub fn get(&self, id: SymbolId) -> Option<&str> {
        self.names.get(id.0 as usize).map(|s| s.as_str())
    }

    /// Clear the reverse lookup map to save memory after lowering is complete.
    pub fn finalize(&mut self) {
        self.name_to_id = None;
    }
}

impl Default for ModuleSymbols {
    fn default() -> Self {
        Self::new()
    }
}
```

### 2. Update `LoweredFunction`

```rust
/// Result of lowering a single function.
#[derive(Clone, Debug)]
pub struct LoweredFunction {
    /// The VInst stream.
    pub vinsts: Vec<VInst>,
    /// Pool of vregs used by Call/Ret VRegSlices.
    /// Indexed by VRegSlice.start.
    pub vreg_pool: Vec<VReg>,
    /// Loop boundary metadata.
    pub loop_regions: Vec<LoopRegion>,
}

impl LoweredFunction {
    /// Create new empty LoweredFunction.
    pub fn new() -> Self {
        Self {
            vinsts: Vec::new(),
            vreg_pool: Vec::new(),
            loop_regions: Vec::new(),
        }
    }

    /// Append vregs to pool, return slice pointing to them.
    pub fn push_vreg_slice(&mut self, vregs: &[VReg]) -> VRegSlice {
        let start = self.vreg_pool.len() as u16;
        let count = vregs.len() as u8;
        self.vreg_pool.extend_from_slice(vregs);
        VRegSlice { start, count }
    }
}
```

### 3. Create `LoweredModule`

```rust
/// Result of lowering an entire module.
#[derive(Clone, Debug)]
pub struct LoweredModule {
    /// Lowered functions.
    pub functions: Vec<LoweredFunction>,
    /// Interned symbol names (for Call targets).
    pub symbols: ModuleSymbols,
}
```

### 4. Update `LowerCtx`

```rust
struct LowerCtx<'a> {
    func: &'a IrFunction,
    ir: &'a IrModule,
    abi: &'a ModuleAbi,
    float_mode: FloatMode,
    out: Vec<VInst>,
    vreg_pool: Vec<VReg>,  // NEW
    next_label: LabelId,
    loop_stack: Vec<LoopFrame>,
    epilogue_label: LabelId,
    loop_regions: Vec<LoopRegion>,
    symbols: &'a mut ModuleSymbols,  // NEW
}
```

### 5. Update `lower_ops` signature

```rust
/// Lower full module.
pub fn lower_module(
    ir: &IrModule,
    abi: &ModuleAbi,
    float_mode: FloatMode,
) -> Result<LoweredModule, LowerError> {
    let mut symbols = ModuleSymbols::new();
    let mut functions = Vec::new();
    
    for func in &ir.functions {
        let lowered = lower_function(func, ir, abi, float_mode, &mut symbols)?;
        functions.push(lowered);
    }
    
    symbols.finalize();
    
    Ok(LoweredModule { functions, symbols })
}

/// Lower single function.
pub fn lower_function(
    func: &IrFunction,
    ir: &IrModule,
    abi: &ModuleAbi,
    float_mode: FloatMode,
    symbols: &mut ModuleSymbols,
) -> Result<LoweredFunction, LowerError> {
    let mut ctx = LowerCtx {
        func,
        ir,
        abi,
        float_mode,
        out: Vec::with_capacity(func.body.len().saturating_mul(2)),
        vreg_pool: Vec::new(),  // NEW
        next_label: 0,
        loop_stack: Vec::new(),
        epilogue_label: 0,
        loop_regions: Vec::new(),
        symbols,  // NEW
    };
    ctx.epilogue_label = ctx.alloc_label();
    ctx.lower_range(0, func.body.len())?;
    ctx.out.push(VInst::Label(ctx.epilogue_label, SRC_OP_NONE));
    
    Ok(LoweredFunction {
        vinsts: ctx.out,
        vreg_pool: ctx.vreg_pool,
        loop_regions: ctx.loop_regions,
    })
}
```

### 6. Update Call lowering

```rust
// In lower_op, Op::Call handling:
Op::Call { callee, args, rets } => {
    let name = resolve_callee_name(ir, *callee)?;
    let target = symbols.intern(&name);  // NEW
    
    // Convert lpir::VReg to native VReg and push to pool
    let native_args: Vec<VReg> = args.iter().map(|v| lower_vreg(*v)).collect();
    let native_rets: Vec<VReg> = rets.iter().map(|v| lower_vreg(*v)).collect();
    
    // Build slices
    let arg_slice = VRegSlice::new(
        ctx.vreg_pool.len() as u16,
        native_args.len() as u8,
    );
    ctx.vreg_pool.extend_from_slice(&native_args);
    
    let ret_slice = VRegSlice::new(
        ctx.vreg_pool.len() as u16,
        native_rets.len() as u8,
    );
    ctx.vreg_pool.extend_from_slice(&native_rets);
    
    Ok(VInst::Call {
        target,
        args: arg_slice,
        rets: ret_slice,
        callee_uses_sret: false, // TODO: compute from ABI
        src_op,
    })
}
```

### 7. Update Return lowering

```rust
// In lower_op, Op::Return handling:
Op::Return { vals } => {
    let native_vals: Vec<VReg> = vals.iter().map(|v| lower_vreg(*v)).collect();
    let val_slice = VRegSlice::new(
        ctx.vreg_pool.len() as u16,
        native_vals.len() as u8,
    );
    ctx.vreg_pool.extend_from_slice(&native_vals);
    
    Ok(VInst::Ret {
        vals: val_slice,
        src_op,
    })
}
```

## Validate

```bash
cargo check -p lpvm-native --lib
```

Check for compile errors. Full test pass comes after updating defs/uses.
