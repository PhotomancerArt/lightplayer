# Phase 3: builtin/

## Scope

Create `const/builtin/` files for spec-listed builtins in const init.

## Code Organization

- One file per builtin category
- 1–2 representative tests per builtin in each file

## Implementation Details

1. **geometric.glsl** — length, dot, normalize:
   - `const float L = length(vec2(1.0, 0.0));` → 1.0
   - `const float D = dot(vec2(1,0), vec2(0,1));` → 0.0

2. **trig.glsl** — radians, degrees, sin, cos, asin, acos:
   - `const float R = radians(180.0);` → π
   - `const float S = sin(3.14159/2.0);` → ~1.0

3. **exp.glsl** — pow, exp, log, exp2, log2, sqrt, inversesqrt:
   - `const float S = sqrt(4.0);` → 2.0
   - `const float P = pow(2.0, 3.0);` → 8.0

4. **common.glsl** — abs, sign, floor, trunc, round, ceil, mod, min, max, clamp:
   - `const float A = abs(-1.5);` → 1.5
   - `const float M = min(1.0, 2.0);` → 1.0

All run directives: `[expect-fail]`.

## Validate

```
just filetest const/builtin/
```
