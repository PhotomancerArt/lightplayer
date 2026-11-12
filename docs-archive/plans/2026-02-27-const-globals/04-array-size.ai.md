# Phase 4: array-size/

## Scope

Create `const/array-size/` files by migrating from `array/` and splitting where needed.

## Code Organization

- Focus on const as constant integral expression for array dimensions

## Implementation Details

1. **const-int.glsl** — Migrate from `array/declare-const-size.glsl`:
   - `const int SIZE = 5; float arr[SIZE];`
   - `const uint U_SIZE = 3u; int arr2[U_SIZE];`
   - Global and local const

2. **const-expr.glsl** — Migrate from `array/declare-const-expression.glsl`:
   - `const int ADD_SIZE = 2 + 3; float arr[ADD_SIZE];`
   - Literal expressions: 2+3, 10-2, 2*3, etc.

3. **local.glsl** — Migrate from `array/phase/8-constant-expressions.glsl`:
   - Local `const int n = 5; int arr[n];`
   - `int arr[3+2];` literal expr

4. **multidim.glsl** — Migrate from `array/declare-multidim-const.glsl`:
   - `const int ROWS = 3; const int COLS = 2; float arr[ROWS][COLS];`

5. **struct-field.glsl** — Migrate from `array/struct-array-const-size.glsl`:
   - Struct fields with const-sized arrays

6. **param.glsl** — Migrate from `array/param-array-const-size.glsl`:
   - Function params with const array size

Delete source files from array/ after migration.

## Validate

```
just filetest const/array-size/
```
