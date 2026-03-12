# Phase 1: Lightweight Float Declarations

## Problem

The float `JITModule` exists only so that `declare_func_in_func` can be called
during CLIF generation. This is the single call site:

```
// frontend/codegen/expr/function.rs:875
let func_ref = ctx.gl_module.module_mut_internal()
    .declare_func_in_func(func_id, ctx.builder.func);
```

A full JITModule costs ~17 KB (ISA, declarations HashMap, memory provider,
symbol lookup table). All we actually need from it is:

```rust
// What declare_func_in_func does internally:
fn declare_func_in_func(&self, func_id: FuncId, func: &mut Function) -> FuncRef {
    let decl = &self.declarations.functions[func_id];
    let signature = func.import_signature(decl.signature.clone());
    let user_name_ref = func.declare_imported_user_function(UserExternalName {
        namespace: 0,
        index: func_id.as_u32(),
    });
    let colocated = decl.linkage.is_final();
    func.import_function(ExtFuncData {
        name: ExternalName::user(user_name_ref),
        signature,
        colocated,
    })
}
```

That's: a Signature, a Linkage, and a FuncId. No JIT machinery needed.

## Fix

Create a lightweight `DeclarationsOnlyModule` that stores just what's needed:

```rust
struct FuncDecl {
    signature: Signature,
    linkage: Linkage,
}

struct DeclarationsOnlyModule {
    functions: HashMap<FuncId, FuncDecl>,
    pointer_type: Type,
}
```

It needs two methods:
- `declare_function(name, linkage, sig) -> FuncId` — assigns sequential FuncIds
- `declare_func_in_func(func_id, func) -> FuncRef` — does the same thing as above

CodegenContext uses `gl_module` for exactly two things:
- `gl_module.module_internal().isa().pointer_type()` — 13 call sites in codegen/
- `gl_module.module_mut_internal().declare_func_in_func(...)` — 1 call site

Option A: Make CodegenContext generic over a trait that provides these two
operations, then implement it for both GlModule<M> and DeclarationsOnlyModule.

Option B: Change CodegenContext to take `pointer_type: Type` as a field and
a closure/callback for `declare_func_in_func`. Simpler but less clean.

Option C: Wrap DeclarationsOnlyModule in a GlModule-like struct and make
CodegenContext use it. Most disruptive but keeps the existing pattern.

Recommended: Option A with a small trait:

```rust
pub trait ModuleContext {
    fn pointer_type(&self) -> Type;
    fn declare_func_in_func(&mut self, func_id: FuncId, func: &mut Function) -> FuncRef;
    fn get_builtin_func_ref(
        &mut self,
        builtin: BuiltinId,
        func: &mut Function,
    ) -> Result<FuncRef, GlslError>;
}
```

Implement for `GlModule<M>` (delegates to inner module) and for
`DeclarationsOnlyModule` (uses its HashMap).

CodegenContext becomes `CodegenContext<'a, C: ModuleContext>` instead of
`CodegenContext<'a, M: Module>`.

## Scope

Files to change:
- New: `backend/module/declarations_only.rs` — the lightweight module
- `backend/module/gl_module.rs` — implement ModuleContext for GlModule<M>
- `frontend/codegen/context.rs` — change CodegenContext generic parameter
- `frontend/codegen/expr/function.rs` — use trait method instead of module_mut
- `frontend/codegen/lp_lib_fns.rs` — use pointer_type() from trait
- `frontend/codegen/lpfx_fns.rs` — same
- `frontend/codegen/lvalue/read.rs` — same
- `frontend/codegen/lvalue/write.rs` — same
- `frontend/codegen/expr/component.rs` — same
- `frontend/mod.rs` — streaming path creates DeclarationsOnlyModule instead of
  float JITModule

Builtins also need to be declared in DeclarationsOnlyModule. The streaming path
currently calls `declare_builtins` on the float module. The lightweight module
needs the same FuncId→Signature mapping for builtins.

## Expected savings

~17 KB from eliminating the float JITModule. This is the single largest win
and the prerequisite for streaming to be net-positive.

## Risk

Medium. Touches the codegen context generics throughout the codegen layer.
The trait is simple but the type parameter change propagates through many
functions. Worth checking that the batch path (which still uses GlModule<M>)
compiles and passes tests.
