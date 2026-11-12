# Phase 7: Remove Old Files and Cleanup

## Scope

Remove migrated files from `global/` and `array/`, verify no orphans, fix any path references.

## Code Organization

- Ensure global/ and array/ no longer contain const-specific files that were migrated

## Implementation Details

1. **Remove from global/**:
   - const-expression.glsl (split in phase 2)
   - const-must-init.glsl (phase 1)
   - const-readonly.glsl (phase 1)
   - declare-const.glsl (phase 5)
   - initialize-const.glsl (phase 2)
   - edge-const-write-error.glsl (phase 1)

2. **Remove from array/**:
   - declare-const-size.glsl (phase 4)
   - declare-const-expression.glsl (phase 4)
   - declare-multidim-const.glsl (phase 4)
   - struct-array-const-size.glsl (phase 4)
   - param-array-const-size.glsl (phase 4)
   - phase/8-constant-expressions.glsl (phase 4)

3. **Keep in global/**: access-read.glsl (qualifier matrix; const is one of many)

4. **Grep for references**: Any test discovery, docs, or scripts that hardcode these paths

## Validate

```
just filetest
```

Full filetest run passes. Const tests live under `const/`.
