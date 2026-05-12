# P1 — LPIR sret marker

Plan dir: `docs/plans/2026-04-22-lp-shader-aggregates-m1-pointer-abi-foundation/`
Read first: `00-design.md`, `00-notes.md` (Q3 records the decision).
Parallel with: P2 (touches independent files in `lps-frontend/`).

## Scope of phase

Add an explicit "this function returns its aggregate result via a hidden
first pointer arg" marker to the LPIR. After this phase:

- `IrFunction` has a new field `pub sret_arg: Option<VReg>`.
- `ImportDecl` has a new field `pub sret: bool`.
- `FunctionBuilder` exposes `add_sret_param()` that allocates the sret
  pointer VReg at `%(vmctx + 1)`.
- VReg numbering and accessors (`user_param_vreg`, `total_param_slots`)
  account for the optional hidden sret slot.
- The text printer emits `sret %N, ` as the first parameter when set.
- The text parser accepts the same syntax.
- `validate.rs` enforces the sret invariants (return_types empty when
  set; sret VReg type is `IrType::Pointer`; ImportDecl's first param is
  Pointer when `sret` is true).

**Out of scope:**

- Frontend lowering changes (P3).
- Backend changes that read `sret_arg` (P4–P7).
- Any test of end-to-end aggregate calls (P9).

This phase is purely a representation change in `lp-shader/lpir/`. It
must leave the workspace compiling and all existing tests green — no
caller of the new field should exist yet.

## Code organization reminders

- Keep changes scoped to `lp-shader/lpir/src/` (and its tests).
- Add the new field with a sensible default (`None` / `false`) so all
  existing constructions still work.
- Place new helpers in the same files as the structs they serve.
- Don't add dead helpers. If a method only makes sense once P3+ lands,
  add it then.
- Add tests in `lp-shader/lpir/src/tests/` if there's a logical place;
  otherwise at the bottom of `lpir_module.rs` / `print.rs` /
  `parse.rs` / `validate.rs`.

## Sub-agent reminders

- Do **not** commit. The plan commits at the end as one unit.
- Stay strictly within `lp-shader/lpir/`. Do not touch `lps-frontend/`,
  `lpvm/`, `lpvm-*/`, or filetests.
- Do **not** add `#[allow(...)]` to silence warnings — fix them.
- Do **not** disable, skip, or weaken existing tests. Existing
  round-trip tests must stay green; you may add new ones.
- If something blocks completion, stop and report.
- Report back: files changed, validation output, deviations.

## Implementation details

### 1. `IrFunction` and `ImportDecl` field additions

`lp-shader/lpir/src/lpir_module.rs`:

```rust
pub struct ImportDecl {
    pub module_name: String,
    pub func_name: String,
    pub param_types: Vec<IrType>,
    pub return_types: Vec<IrType>,
    pub lpfn_glsl_params: Option<String>,
    pub needs_vmctx: bool,
    /// When true, the *first* entry of `param_types` is a hidden
    /// `IrType::Pointer` sret destination. Callers must allocate the
    /// destination buffer and pass its address as the first arg
    /// (immediately after vmctx if `needs_vmctx`); the callee writes
    /// its return value into that buffer and the actual `return_types`
    /// is empty.
    pub sret: bool,
}

pub struct IrFunction {
    pub name: String,
    pub is_entry: bool,
    pub vmctx_vreg: VReg,
    /// User-visible parameter count (excluding VMContext **and** sret).
    pub param_count: u16,
    pub return_types: Vec<IrType>,
    /// When `Some(vreg)`, the function returns its aggregate value via
    /// a hidden `IrType::Pointer` parameter at `vreg`. `vreg` lives at
    /// `VReg(vmctx_vreg.0 + 1)`. `return_types` is empty in this case.
    pub sret_arg: Option<VReg>,
    pub vreg_types: Vec<IrType>,
    pub slots: Vec<SlotDecl>,
    pub body: Vec<LpirOp>,
    pub vreg_pool: Vec<VReg>,
}
```

Update accessors:

```rust
impl IrFunction {
    /// Number of hidden VRegs preceding user params (vmctx + optional sret).
    #[inline]
    pub fn hidden_param_slots(&self) -> u32 {
        1 + self.sret_arg.is_some() as u32
    }

    /// VReg for user parameter `user_index` (`0` = first GLSL parameter).
    #[inline]
    pub fn user_param_vreg(&self, user_index: u16) -> VReg {
        debug_assert!(user_index < self.param_count);
        VReg(self.vmctx_vreg.0 + self.hidden_param_slots() + u32::from(user_index))
    }

    /// Total parameter slots including VMContext **and** sret (`hidden + param_count`).
    #[inline]
    pub fn total_param_slots(&self) -> u16 {
        (self.hidden_param_slots() as u16).saturating_add(self.param_count)
    }
}
```

Find every existing reader of `param_count` / `total_param_slots` /
`user_param_vreg` and confirm none of them need updating beyond what the
accessor change already provides. Grep targets: `param_count`,
`user_param_vreg`, `total_param_slots`, `vmctx_vreg`. Most callers should
be unaffected because they go through the accessors.

### 2. `FunctionBuilder` support

`lp-shader/lpir/src/builder.rs`:

Add a new field tracking sret state and a method to allocate it:

```rust
pub struct FunctionBuilder {
    name: String,
    is_entry: bool,
    return_types: Vec<IrType>,
    sret_arg: Option<VReg>,         // NEW
    param_count: u16,
    vreg_types: Vec<IrType>,
    slots: Vec<SlotDecl>,
    body: Vec<LpirOp>,
    vreg_pool: Vec<VReg>,
    next_vreg: u32,
    next_slot: u32,
    block_stack: Vec<BlockEntry>,
}

impl FunctionBuilder {
    /// Allocate the hidden sret pointer parameter. **Must be called
    /// before any [`Self::add_param`].** Sets `IrFunction::sret_arg`
    /// and reserves `VReg(vmctx + 1)` for the returned pointer.
    pub fn add_sret_param(&mut self) -> VReg {
        assert!(self.sret_arg.is_none(), "add_sret_param called twice");
        assert_eq!(
            self.param_count, 0,
            "add_sret_param must be called before any user params (next_vreg={})",
            self.next_vreg
        );
        let v = VReg(self.next_vreg);
        self.next_vreg += 1;
        self.vreg_types.push(IrType::Pointer);
        self.sret_arg = Some(v);
        v
    }
}
```

In `FunctionBuilder::new`, initialise `sret_arg: None` (and assert in
`finish` that if `sret_arg.is_some()` then `return_types.is_empty()`):

```rust
pub fn finish(mut self) -> IrFunction {
    assert!(self.block_stack.is_empty(), "...");
    if self.sret_arg.is_some() {
        assert!(
            self.return_types.is_empty(),
            "FunctionBuilder::finish: sret functions must have empty return_types"
        );
    }
    IrFunction {
        name: core::mem::take(&mut self.name),
        is_entry: self.is_entry,
        vmctx_vreg: VMCTX_VREG,
        param_count: self.param_count,
        return_types: self.return_types,
        sret_arg: self.sret_arg,
        vreg_types: self.vreg_types,
        slots: self.slots,
        body: self.body,
        vreg_pool: self.vreg_pool,
    }
}
```

Equivalent helper for `ModuleBuilder` / `ImportDecl`: when constructing
an `ImportDecl` directly (in tests or downstream code), `sret` defaults
to `false`. No new `add_import_sret(...)` constructor needed in this
phase — leave it for the consumers in P4–P7.

### 3. Text printer — `print.rs`

Update `print_function` to emit `sret %N, ` immediately after the opening
`(`, when `func.sret_arg.is_some()`. Example output:

```
func @returns_array(sret %1) -> () {
  ; body uses %1 as the destination pointer
  return
}
```

For functions with both sret and user params:

```
func @returns_struct(sret %1, %2:f32) -> () {
  ...
}
```

For `ImportDecl`, emit a `sret ` prefix on the first param when
`imp.sret`:

```
import @vm::__lp_some_sret(sret ptr, i32) -> ()
```

(The exact word `sret ` precedes the type; the type is always `ptr`.)

Update `print_function`'s parameter loop:

```rust
fn print_function(out: &mut String, func: &IrFunction, module: &LpirModule) {
    if func.is_entry { let _ = write!(out, "entry "); }
    let _ = write!(out, "func @{}(", func.name);
    let mut first = true;
    if let Some(sret) = func.sret_arg {
        let _ = write!(out, "sret {sret}");
        first = false;
    }
    let vm = func.vmctx_vreg.0 as usize;
    for i in 0..func.param_count as usize {
        if !first { let _ = write!(out, ", "); }
        first = false;
        let j = vm + func.hidden_param_slots() as usize + i;
        let _ = write!(out, "{}:{}", VReg(j as u32), func.vreg_types[j]);
    }
    let _ = write!(out, ")");
    if !func.return_types.is_empty() {
        let _ = write!(out, " -> ");
        print_return_types(out, &func.return_types);
    }
    // ... rest unchanged
}
```

Mirror change in `print_import` for `imp.sret`.

### 4. Text parser — `parse.rs`

Locate the function header parser (search for `"func "` or
`parse_function`). Accept `sret %N` as an optional first parameter. When
present:

- Allocate it to `IrFunction::sret_arg`.
- Record the VReg type as `IrType::Pointer`.
- Subsequent parameters parse as user params (incrementing `param_count`,
  not `sret_arg`).

For imports, accept `sret ptr` (or `sret <ty>`) as an optional first
param-type when `imp.sret = true`. Reject if not pointer-typed.

Add round-trip tests in `lp-shader/lpir/src/tests/all_ops_roundtrip.rs`
or a sibling file:

```rust
#[test]
fn sret_function_roundtrip() {
    let src = "\
func @ret_arr(sret %1) -> () {
  return
}
";
    let module = parse_module(src).expect("parse");
    let printed = print_module(&module);
    assert_eq!(src.trim_end(), printed.trim_end());
    let func = module.functions.values().next().unwrap();
    assert_eq!(func.sret_arg, Some(VReg(1)));
    assert!(func.return_types.is_empty());
    assert_eq!(func.param_count, 0);
    assert_eq!(func.vreg_types[1], IrType::Pointer);
}

#[test]
fn sret_function_with_user_params_roundtrip() {
    let src = "\
func @ret_arr(sret %1, %2:f32, %3:i32) -> () {
  return
}
";
    let module = parse_module(src).expect("parse");
    let printed = print_module(&module);
    assert_eq!(src.trim_end(), printed.trim_end());
    let func = module.functions.values().next().unwrap();
    assert_eq!(func.sret_arg, Some(VReg(1)));
    assert_eq!(func.param_count, 2);
    assert_eq!(func.user_param_vreg(0), VReg(2));
    assert_eq!(func.user_param_vreg(1), VReg(3));
}

#[test]
fn sret_import_roundtrip() {
    let src = "\
import @vm::ret_thing(sret ptr) -> ()
";
    let module = parse_module(src).expect("parse");
    let printed = print_module(&module);
    assert_eq!(src.trim_end(), printed.trim_end());
    assert!(module.imports[0].sret);
}
```

### 5. Validation — `validate.rs`

Find the existing function-validation pass (`validate_module` or
similar). Add invariants:

- If `func.sret_arg.is_some()`:
  - `func.return_types.is_empty()`
  - `func.vreg_types[sret_vreg.0 as usize] == IrType::Pointer`
  - `sret_vreg.0 == func.vmctx_vreg.0 + 1`
  - All `LpirOp::Return { values }` in the body have `values.count == 0`.
- If `imp.sret`:
  - `imp.return_types.is_empty()`
  - `imp.param_types.first() == Some(&IrType::Pointer)`

Return descriptive errors. Add at least one positive and one negative
test for each invariant (e.g. construct an `IrFunction { sret_arg:
Some(_), return_types: vec![IrType::I32], .. }` and confirm
`validate_module` returns an error).

### 6. Backwards compatibility

Every existing `IrFunction { ... }` literal in tests / parsers / builders
must initialise `sret_arg: None`. Likewise for `ImportDecl { .. }` →
`sret: false`. Find them with grep:

```
rg "IrFunction \{" lp-shader/
rg "ImportDecl \{"  lp-shader/
```

Most should already go through `FunctionBuilder::finish` /
`ModuleBuilder::add_import` / a parser. Direct literals are likely test
helpers in `lp-shader/lpir/src/tests/`.

## Validate

From the repo root:

```
cargo check -p lpir
cargo test  -p lpir
just check
```

`just check` (`fmt-check + clippy`) must be green. Existing LPIR
round-trip tests must remain green. New sret round-trip tests pass.

## Done when

- All listed file edits land.
- New round-trip + validation tests pass.
- `cargo check -p lpir` and `cargo test -p lpir` are green.
- `just check` is green.
- No new `#[allow(...)]` introduced.
- Workspace still builds (downstream crates haven't started using the
  new field yet, but adding a field with a default value should be
  source-compatible with existing pattern matches via `..`).
