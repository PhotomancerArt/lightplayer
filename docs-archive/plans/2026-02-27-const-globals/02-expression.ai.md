# Phase 2: expression/

## Scope

Create `const/expression/` files by splitting from `global/const-expression.glsl` and `global/initialize-const.glsl`.

## Code Organization

- Split by constant expression form (spec §4.3.3.1)
- One form per file

## Implementation Details

1. **literal.glsl** — Literal values as const init:
   - `const float PI = 3.14159;`, `const int N = 42;`, `const bool B = true;`
   - vec2/3/4, mat2 with literal constructors

2. **operators.glsl** — Binary operators on const:
   - `const float T = 2.0 * PI;`, `PI / 2.0`, `ANSWER * 2`
   - +, -, *, /, %

3. **constructors.glsl** — Constructors with const args:
   - `const vec2 UNIT = vec2(1.0, 0.0);`, `UNIT_VECTOR * 2.0`
   - mat2 with const refs

4. **reference.glsl** — Const references other const (ordering):
   - `const float B = A + 1.0;` where A is earlier const
   - Nested: `const float C = B / 2.0;`

5. **unary.glsl** — Unary minus:
   - `const int N = -42;`, `const float F = -PI;`

All run directives: `[expect-fail]`.

## Validate

```
just filetest const/expression/
```
