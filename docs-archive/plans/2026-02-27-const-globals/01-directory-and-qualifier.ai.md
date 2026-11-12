# Phase 1: Directory Structure and qualifier/

## Scope

Create `const/` directory structure and `qualifier/` files by migrating from `global/`.

## Code Organization

- One concept per file
- Header block with spec reference

## Implementation Details

1. Create directories:
   - `filetests/const/qualifier/`
   - `filetests/const/expression/`
   - `filetests/const/builtin/`
   - `filetests/const/array-size/`
   - `filetests/const/scope/`
   - `filetests/const/errors/`

2. Create `qualifier/must-init.glsl` — migrate from `global/const-must-init.glsl`:
   - Spec §4.3.3: const must be initialized
   - Scalar types (float, int, uint, bool), vec2/3/4, mat2
   - Keep tests minimal; all run directives `[expect-fail]`

3. Create `qualifier/readonly.glsl` — migrate from `global/const-readonly.glsl`:
   - Const is read-only; reading allowed

4. Create `qualifier/write-error.glsl` — migrate from `global/edge-const-write-error.glsl`:
   - Tests read path (write-to-const would be error; commented out)
   - Valid code that reads from const

5. Delete from global/ after migration:
   - `const-must-init.glsl`
   - `const-readonly.glsl`
   - `edge-const-write-error.glsl`

## Validate

```
just filetest const/qualifier/
```

All three files should run (expect-fail on run directives).
