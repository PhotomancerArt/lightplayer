# Phase 4: Delete Dead Files and Fix Stale Comments

## Dead file

Delete `frontend/codegen/lp_lib_fns.rs`. It is not included in `mod.rs`
(superseded by `lpfn_fns.rs`) and contains stale transform references.

## Stale comments to fix

### `frontend/codegen/lpfn_fns.rs`
- **Lines 231–234**: Doc on `get_lpfn_testcase_call` says "These are
  converted to q32 builtins by the transform." → Update to reflect that
  this path is only used in float mode; Q32 uses builtins directly.
- **Lines 253–254**: "The transform will convert this to q32 when
  processing the call" → Remove, replace with note that this is the
  float-only path.

### `frontend/codegen/builtins/helpers.rs`
- **Line 30**: "converted to q32 by transform when applicable" → Remove
  the "when applicable" clause. The float path stays float; there is no
  transform.

### `frontend/glsl_compiler.rs`
- **Lines 69–70**: "Declare all user functions with FLOAT signatures (no
  conversion)" → Signatures are numeric-mode-dependent now.
- **Lines 137–138**: Same pattern for main function.
- **Lines 224, 292**: Same in object/emulator path.

### `frontend/mod.rs`
- **Line ~448**: "transformations already applied" → No transformations;
  this is direct emission.

### `frontend/codegen/numeric.rs`
- **Line 31**: "use `todo!()` for now" → Should say `unreachable!()`,
  since Plan C resolved these with builtin calls and the strategy methods
  now use `unreachable!()`.
