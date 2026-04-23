# P4 — `lpvm-cranelift`: sret driven by `IrFunction.sret_arg`

Plan dir: `docs/plans/2026-04-22-lp-shader-aggregates-m1-pointer-abi-foundation/`
Read first: `00-design.md`, `00-notes.md`.
Depends on: P1 (LPIR sret marker), P3 (frontend now emits aggregate
pointer args + sret).
Parallel with: P5 (`lpvm-native`), P7 (`lpvm-wasm`).

## Scope of phase

Drive Cranelift's struct-return signature from the explicit
`IrFunction::sret_arg` / `ImportDecl::sret` markers, instead of the
"if return scalar count > N then sret" heuristic that exists today.
Existing infrastructure for `ArgumentPurpose::StructReturn` is already
in place (`lp-shader/lpvm-cranelift/src/emit/call.rs` shows callers
allocating a stack slot and passing its address; callee codegen reads
from the hidden first arg). We are *only* changing what triggers it.

Concretely:

- `signature_uses_struct_return` (or equivalent helper in
  `lp-shader/lpvm-cranelift/src/emit/mod.rs`) becomes
  `func.sret_arg.is_some()` (and the import-side equivalent reads
  `imp.sret`).
- The signature-construction path adds a leading parameter with
  `ArgumentPurpose::StructReturn` exactly when `sret_arg` is set, with
  CL `Type` = native pointer (`module.target_config().pointer_type()`).
- The caller side in `emit/call.rs` continues to allocate a stack slot
  and pass its address as the hidden first arg — the trigger source
  changes, the mechanism stays.
- Callee side: the body's first non-vmctx VReg (the LPIR `sret_arg`) is
  bound to the corresponding CL block parameter. Find the existing
  binding logic and adapt it to use `func.hidden_param_slots()` instead
  of hard-coded `1` for the vmctx skip.
- All scalar-count-based heuristics for sret are deleted.

**Out of scope:**

- Any change to the Cranelift instruction emit for `Memcpy`, `Load`,
  `Store`, `Call` — those already work.
- Filetests (P9).
- Host marshalling (P6, P8).

## Code organization reminders

- Keep changes scoped to `lp-shader/lpvm-cranelift/src/emit/`.
- Don't introduce new public items unless the new sret trigger needs
  one. Prefer in-place updates to existing helpers.
- Mark anything truly transitional with `// TODO(M1):` so P10 can grep
  it.

## Sub-agent reminders

- Do **not** commit.
- Stay strictly within `lp-shader/lpvm-cranelift/`. Do not edit
  `lps-frontend/`, `lpvm-native/`, `lpvm-wasm/`, `lpvm-emu/`, or
  filetests.
- Do **not** suppress warnings or add `#[allow(...)]`.
- Do **not** weaken or `#[ignore]` tests.
- If callee-side binding turns out to need broader changes than
  documented here, **stop and report**.
- Report back: files changed, validation output, deviations.

## Implementation details

### 1. Inventory the existing sret machinery

Read these files and confirm what's already there:

- `lp-shader/lpvm-cranelift/src/emit/mod.rs` — has
  `signature_uses_struct_return` (or similarly named function); it
  currently decides sret based on `ir_func.return_types`'s scalar
  count. Identify where it's called.
- `lp-shader/lpvm-cranelift/src/emit/call.rs` — has
  `signature_for_callee` (or similar) and the caller-side stack-slot
  allocation + `ArgumentPurpose::StructReturn` arg push. Identify how
  it currently knows whether the callee uses sret (it likely calls back
  into the same `signature_uses_struct_return` heuristic).
- Wherever the callee's body wires LPIR VRegs to CL block params (look
  for a function that maps `IrFunction::vreg_types` to CL block param
  types).

Confirm: today's heuristic applies sret when the return count exceeds
the first-class register-return budget (e.g. > 4 i32-equivalent
returns). After P3, **no LPIR function for an aggregate has any returns
at all** — the heuristic returns "false" for sret functions, breaking
them. That is exactly what this phase fixes.

### 2. Replace the trigger

```rust
// Before:
fn signature_uses_struct_return(func: &IrFunction) -> bool {
    // ... scalar-count heuristic ...
}

// After:
fn signature_uses_struct_return(func: &IrFunction) -> bool {
    func.sret_arg.is_some()
}
```

For imports / external signatures (used when emitting calls to
imports): replace the heuristic with `imp.sret`. If the existing helper
takes a generic `&[IrType]` slice (return types) rather than the full
`IrFunction` / `ImportDecl`, change its signature to take `bool sret`
or the full struct, and propagate.

### 3. Signature construction

The cranelift `Signature` builder for a function should now produce, in
order:

1. The vmctx pointer param (existing — usually first).
2. The sret pointer param with `ArgumentPurpose::StructReturn` (new
   trigger — exists when `sret_arg.is_some()`).
3. User params (one CL param per LPIR param after vmctx and sret;
   pointer types pass as native pointer width).

Type for sret arg:

```rust
let ptr_ty = module.target_config().pointer_type();
let mut sret_arg = AbiParam::new(ptr_ty);
sret_arg.purpose = ArgumentPurpose::StructReturn;
sig.params.push(sret_arg);
```

Returns:

```rust
if !signature_uses_struct_return(func) {
    for ty in &func.return_types {
        sig.returns.push(AbiParam::new(ir_type_to_cl(ty, ptr_ty)));
    }
} else {
    debug_assert!(func.return_types.is_empty(),
        "sret function must have empty return_types");
    // Cranelift convention: also emit a single i-pointer return that
    // mirrors the sret pointer? Check current code — many configs do.
    sig.returns.push(AbiParam::special(ptr_ty, ArgumentPurpose::StructReturn));
}
```

Inspect the existing `emit/call.rs` to see whether Cranelift here
expects the sret to also appear in `returns`. The existing code path
(currently triggered by the heuristic) is your reference — match
exactly what it does for scalar count > N. The only change is the
trigger source.

### 4. Callee-side VReg → CL block param binding

The function-prologue lowering walks `IrFunction::vreg_types` and
binds each VReg to its CL block param. With `sret_arg`, the binding
order must remain:

- `vmctx_vreg` ↔ block param 0 (vmctx).
- `sret_arg` (when set) ↔ block param 1 (sret pointer).
- User params ↔ block params `2..2+param_count` (or `1..1+param_count`
  when no sret).

Replace any hard-coded `let user_start = 1;` with
`let user_start = func.hidden_param_slots() as usize;`.

If the existing binding uses an enumerate-pattern walking
`func.vreg_types[..1 + func.param_count as usize]`, change it to
`..(func.hidden_param_slots() as usize + func.param_count as usize)`.

### 5. Caller-side: passing sret

`emit/call.rs` already implements:

1. Allocate a CL `StackSlot` of size = sum of return scalar widths.
2. Take its address via `stack_addr`.
3. Push the address as the first call arg.
4. After the call, load each return component back from the slot.

For sret functions emitted by P3, **steps 1 and 4 must come from the
LPIR side**, not from cranelift inferring them. The frontend already
allocates the dest slot and stores the slot's address into a VReg; the
LPIR `Call.args` operand list begins `[vmctx, sret_addr, ...]`. So
cranelift's caller-side path becomes the trivial:

- For each LPIR call arg (in order), pass the corresponding CL value to
  the call (the sret pointer is just one of those args; cranelift sees
  it as a normal pointer).
- After the call, **do not** load anything back — the LPIR already
  inserts subsequent `Load { base: sret_addr, offset }` ops if/when
  the result is read.

If the existing path also performed load-back fixups for the sret slot,
gate those out when the call is "to a function whose LPIR signature
already includes sret" (i.e. P3-emitted). Check the existing code; the
right way to express this might be "the call's first non-vmctx arg has
purpose `StructReturn` *and* the LPIR Call.args includes that arg
explicitly".

### 6. Tests

Add a small unit test in `lp-shader/lpvm-cranelift/` that takes a
hand-built LPIR module:

- Function `@returns_arr4(sret %1) -> ()` whose body stores 4 floats
  into `%1` and returns void.
- Function `@caller(... %0:vmctx)` that allocates a slot, calls
  `@returns_arr4` with sret addressing the slot, then loads the 4
  floats back and uses them.

Compile to a Cranelift function and assert the produced signature has
`ArgumentPurpose::StructReturn` on param 1 and that the call site
threads the sret pointer through. If the existing test harness for
this crate doesn't easily support hand-built LPIR, add a TODO and
defer the unit test to P9 (call this out in the report).

## Validate

```
cargo check -p lpvm-cranelift
cargo test  -p lpvm-cranelift
just test-glsl                 # cranelift is in this set
```

`just test-glsl-filetests` may fail behaviourally because filetests
haven't been re-baselined yet (P9). That is expected. Report any
failure that looks like a *codegen* bug rather than a CHECK-line
mismatch.

## Done when

- Sret trigger reads `sret_arg` / `imp.sret` everywhere that previously
  used the scalar-count heuristic.
- Heuristic is deleted (no more "if return count > N then sret").
- Callee binding uses `func.hidden_param_slots()`.
- Caller-side does not duplicate the LPIR's sret-slot machinery.
- `cargo test -p lpvm-cranelift` is green.
- `just check` is green for this crate.
- No new `#[allow(...)]`; no `#[ignore]`d tests.
