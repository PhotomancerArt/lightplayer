# Array Implementation Phases

## Phase 1: Foundation
Basic 1D scalar arrays with literal int sizes, stack allocation, pointer-based storage, basic read/write access (`arr[i] = x`, `x = arr[i]`).

## Phase 2: Bounds Checking
Runtime bounds checking for array reads and writes using `trapnz`.

## Phase 3: Initialization
Array initializer lists `{1.0, 2.0, 3.0}` with full and partial initialization, and unsized arrays with size inferred from initializer (`float[] = {1.0, 2.0, 3.0}`).

## Phase 4: Vector/Matrix Element Arrays
Arrays of vectors and matrices (`vec4 arr[5]`, `mat3 arr[3]`) with component access (`arr[i].x`).

## Phase 5: Multi-dimensional Arrays
Nested arrays (`float[5][3]`) with multi-dimensional indexing (`arr[i][j]`).

## Phase 6: Verify All Operators
Verify increment/decrement, compound assignment, binary/unary operations on array elements work correctly (`arr[i]++`, `arr[i] += x`, `arr[i] + x`, etc.). Should work automatically via LValue pattern, but needs verification and testing.

## Phase 7: Function Parameters
Arrays as function parameters and return values (pass by pointer).

## Phase 8: Constant Expression Array Sizes
Support constant expressions for array sizes (`const int n = 5; float arr[n]`, `float arr[5+3]`).

## Phase 9: Array Constructors
Array constructor syntax (`float[5](1.0, 2.0, 3.0, 4.0, 5.0)`).

## Test Files

Each phase will have temporary test files in `lightplayer/crates/lp-glsl-filetests/filetests/arrays/phase/`:
- `phase/1.glsl` - Phase 1 tests
- `phase/2.glsl` - Phase 2 tests
- etc.

These will be removed after implementation is complete, but provide clear success criteria during development.





