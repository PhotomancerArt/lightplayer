# Phase 3: Lower Op::Call to VInst::Call

## Scope of Phase

Update the lowering pipeline to handle `Op::Call`. This involves:
1. Threading `IrModule` and `ModuleAbi` through to `lower_ops` and `lower_op`
2. Resolving `CalleeRef` to function names
3. Detecting if callee uses sret via `ModuleAbi`
4. Creating `VInst::Call` with proper `callee_uses_sret` flag

## Code Organization Reminders

- Place `lower_op` signature changes near the top
- Keep the `Op::Call` match arm with other op lowerings
- Add helper function at the bottom for CalleeRef resolution
- Add tests at the bottom of the test module

## Implementation Details

### File: `lp-shader/lpvm-native/src/lower.rs`

**Update `lower_op` signature:**

```rust
/// Lower one LPIR op. `src_op` is the index in [`IrFunction::body`].
pub fn lower_op(
    op: &Op,
    float_mode: FloatMode,
    src_op: Option<u32>,
    func: &IrFunction,
    ir: &IrModule,           // NEW
    abi: &ModuleAbi,         // NEW
) -> Result<VInst, LowerError> {
```

**Update float builtin lowerings to include new parameters:**

```rust
Op::Fadd { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Call {
    target: SymbolRef {
        name: String::from("__lp_lpir_fadd_q32"),
    },
    args: alloc::vec![*lhs, *rhs],
    rets: alloc::vec![*dst],
    callee_uses_sret: false,
    src_op,
}),
// Same pattern for Fsub, Fmul
```

**Add Op::Call handling (after float ops, before the catch-all):**

```rust
Op::Call { callee, args, results } => {
    // Resolve CalleeRef to function name
    let name = resolve_callee_name(ir, *callee)
        .ok_or_else(|| LowerError::UnsupportedOp {
            description: format!("Call: cannot resolve callee {:?}", callee),
        })?;
    
    // Determine if callee uses sret
    let callee_uses_sret = if let Some(func_abi) = abi.func_abi(&name) {
        func_abi.is_sret()
    } else {
        // Unknown callee (might be external/builtin not in ModuleAbi)
        // Default to false (direct return) - builtins use direct return
        false
    };
    
    // Get args and results from vreg_pool
    let args_slice = func.pool_slice(*args);
    let results_slice = func.pool_slice(*results);
    
    Ok(VInst::Call {
        target: SymbolRef { name },
        args: args_slice.to_vec(),
        rets: results_slice.to_vec(),
        callee_uses_sret,
        src_op,
    })
}
```

**Add helper function at bottom of file:**

```rust
/// Resolve a CalleeRef to a function name.
/// Returns None if the callee index is out of range.
fn resolve_callee_name(ir: &IrModule, callee: lpir::CalleeRef) -> Option<String> {
    let idx = callee.0 as usize;
    let import_count = ir.imports.len();
    
    if idx < import_count {
        // Callee is an import (builtin)
        ir.imports.get(idx).map(|imp| imp.func_name.clone())
    } else {
        // Callee is a local function
        let func_idx = idx - import_count;
        ir.functions.get(func_idx).map(|f| f.name.clone())
    }
}
```

**Update `LowerCtx` to hold IrModule and ModuleAbi:**

```rust
struct LowerCtx<'a> {
    func: &'a IrFunction,
    ir: &'a IrModule,        // NEW
    abi: &'a ModuleAbi,      // NEW
    float_mode: FloatMode,
    out: Vec<VInst>,
    next_label: LabelId,
    loop_stack: Vec<LoopFrame>,
    epilogue_label: LabelId,
}
```

**Update `lower_range` recursive calls to pass ir and abi:**

```rust
// In lower_range, update the recursive calls:
self.lower_range(i + 1, eo)?;  // Already uses self which now has ir and abi
```

**Update `lower_ops` signature and context creation:**

```rust
/// Lower full function body (including if/else and loop control flow).
pub fn lower_ops(
    func: &IrFunction,
    ir: &IrModule,           // NEW
    abi: &ModuleAbi,         // NEW
    float_mode: FloatMode,
) -> Result<Vec<VInst>, LowerError> {
    let mut ctx = LowerCtx {
        func,
        ir,                    // NEW
        abi,                   // NEW
        float_mode,
        out: Vec::with_capacity(func.body.len().saturating_mul(2)),
        next_label: 0,
        loop_stack: Vec::new(),
        epilogue_label: 0,
    };
    // ... rest unchanged
}
```

**Update recursive `lower_range` call to `lower_op`:**

```rust
// Find this line in lower_range:
self.out.push(lower_op(other, self.float_mode, Some(i as u32), self.func)?);

// Update to:
self.out.push(lower_op(other, self.float_mode, Some(i as u32), self.func, self.ir, self.abi)?);
```

### Tests to Add

```rust
#[test]
fn lower_call_direct_return() {
    let mut f = empty_func();
    f.vreg_pool = vec![v(10), v(11), v(12)];  // args and result
    f.body = vec![
        Op::Call {
            callee: lpir::CalleeRef(0),  // first import
            args: VRegRange { start: 0, count: 2 },
            results: VRegRange { start: 2, count: 1 },
        },
    ];
    
    let mut ir = IrModule::new();
    ir.imports.push(lpir::ImportDecl {
        module_name: String::from("builtins"),
        func_name: String::from("add"),
        param_types: vec![lpir::IrType::I32, lpir::IrType::I32],
        return_types: vec![lpir::IrType::I32],
        lpfn_glsl_params: None,
        needs_vmctx: false,
    });
    
    let sig = lps_shared::LpsModuleSig { functions: vec![] };
    let abi = ModuleAbi::from_lps_module_sig(&sig);
    
    let vinsts = lower_ops(&f, &ir, &abi, FloatMode::Q32).expect("lower");
    assert_eq!(vinsts.len(), 2);  // call + epilogue label
    
    match &vinsts[0] {
        VInst::Call { target, args, rets, callee_uses_sret, .. } => {
            assert_eq!(target.name, "add");
            assert_eq!(args, &vec![v(10), v(11)]);
            assert_eq!(rets, &vec![v(12)]);
            assert!(!callee_uses_sret);  // import not in ModuleAbi defaults to false
        }
        other => panic!("expected Call, got {:?}", other),
    }
}

#[test]
fn lower_call_local_function() {
    let f = empty_func();
    
    let mut ir = IrModule::new();
    // Add a local function that callee will reference
    ir.functions.push(IrFunction {
        name: String::from("helper"),
        is_entry: false,
        vmctx_vreg: VReg(0),
        param_count: 1,
        return_types: vec![lpir::IrType::Vec4],  // sret return
        vreg_types: vec![],
        slots: vec![],
        body: vec![],
        vreg_pool: vec![],
    });
    
    // Function that calls helper
    let mut caller = empty_func();
    caller.name = String::from("caller");
    caller.vreg_pool = vec![v(10), v(11), v(12), v(13), v(14)];  // vmctx, arg, 4 results
    caller.body = vec![
        Op::Call {
            callee: lpir::CalleeRef(0),  // first local function (import_count=0)
            args: VRegRange { start: 0, count: 2 },  // vmctx + arg
            results: VRegRange { start: 2, count: 4 },  // vec4
        },
    ];
    ir.functions.push(caller);
    
    let sig = lps_shared::LpsModuleSig {
        functions: vec![
            lps_shared::LpsFnSig {
                name: String::from("helper"),
                return_type: lps_shared::LpsType::Vec4,
                parameters: vec![lps_shared::FnParam {
                    name: String::from("x"),
                    ty: lps_shared::LpsType::Float,
                    qualifier: lps_shared::ParamQualifier::In,
                }],
            },
        ],
    };
    let abi = ModuleAbi::from_lps_module_sig(&sig);
    
    let caller_func = &ir.functions[1];  // caller is second function
    let vinsts = lower_ops(caller_func, &ir, &abi, FloatMode::Q32).expect("lower");
    
    match &vinsts[0] {
        VInst::Call { target, callee_uses_sret, .. } => {
            assert_eq!(target.name, "helper");
            assert!(callee_uses_sret);  // helper returns vec4 which uses sret
        }
        other => panic!("expected Call, got {:?}", other),
    }
}
```

## Validate

```bash
cargo test -p lpvm-native lower_call
cargo check -p lpvm-native
```

Ensure:
- Tests pass
- No compiler warnings
- All existing tests still pass
