# Phase 3: Forward Declarations

## Scope

Fix "undefined function" errors when a function is called before its
definition, even when a prototype (forward declaration) exists.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Analysis

The lowering in `lower.rs` already builds `func_map` by iterating
`naga_module.functions` (lines 26-29) before lowering any bodies. This means
every function handle gets a `CalleeRef` before code generation starts.

The error `undefined function 'add_two_floats'` suggests the issue is
**upstream** of lowering — in how `NagaModule::functions` is populated from
the Naga parse result. If a prototype-only declaration doesn't create a
`Function` entry in `naga::Module::functions`, then the function handle won't
exist until its body is parsed.

## Implementation

### Step 1: Diagnose

Run the failing test with debug output to see which functions are in
`naga_module.functions`:

```bash
DEBUG=1 scripts/glsl-filetests.sh function/declare-prototype.glsl
```

Check whether the Naga module contains `add_two_floats` as a function handle
at all, or if the issue is in how `NagaModule` filters/collects functions.

### Step 2: Fix

**If Naga includes all functions** (including prototype-forward-declared ones):
The bug is in `NagaModule::functions` ordering or filtering. Fix by ensuring
all function handles are collected regardless of whether they appear before or
after their definition.

**If Naga doesn't include prototype-only functions:** The GLSL frontend may
not emit them to `module.functions` until the body is parsed. In that case:

- Check if Naga's GLSL frontend handles prototypes at all.
- If it does, prototype + definition should share the same function handle.
- If it doesn't, we may need to handle this at the `NagaModule` level by
  ensuring definitions are linked to their prototypes.

### Step 3: Handle void return + empty body

The existing code skips functions with empty bodies (line 167-174 in
`lower_stmt.rs`). Verify this doesn't interfere with prototype handling —
a prototype has no body, but its definition (later in source) does.

### Tests

- `function/declare-prototype.glsl` — 4 run cases
- `function/recursive-static-error.glsl` — 2 run cases (non-recursive ones)

## Validate

```bash
cargo test -p lps-frontend -q
scripts/glsl-filetests.sh function/declare-prototype.glsl
scripts/glsl-filetests.sh function/recursive-static-error.glsl
```
