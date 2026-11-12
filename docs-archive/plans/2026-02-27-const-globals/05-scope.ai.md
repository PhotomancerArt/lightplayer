# Phase 5: scope/

## Scope

Create `const/scope/` files for global vs local const.

## Code Organization

- global.glsl: declaration and use at global scope
- local.glsl: const inside functions (distinct from array-size/local which focuses on array dims)

## Implementation Details

1. **global.glsl** — Migrate from `global/declare-const.glsl`:
   - Const declarations by type (float, int, uint, bool, vec2/3/4, mat2)
   - Use in functions: `return PI * 2.0;`

2. **local.glsl** — Local const (inside function body):
   - `float f() { const float x = 1.0; return x; }`
   - Local const used in expressions (not array size — that's array-size/local.glsl)

Delete `global/declare-const.glsl` and `global/initialize-const.glsl` if not already removed.

## Validate

```
just filetest const/scope/
```
