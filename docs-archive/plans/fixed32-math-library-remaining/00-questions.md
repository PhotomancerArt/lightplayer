# Questions for Q32 Math Library - Remaining Functions

## Current State

We have:
- ✅ Infrastructure for q32 math builtins (test helpers, registry, transform conversion)
- ✅ `sin` and `cos` implemented using libfixmath's Taylor series
- ✅ Transform conversion working for both TestCase and User function names
- ✅ Test infrastructure with tolerance support

## Goal

Implement the remaining q32 math functions following the same infrastructure pattern, reordered to respect dependencies:
- **Phase 3**: Tangent (tan) - depends on sin/cos, simplest to implement first
- **Phase 4**: Inverse trig (asin, acos, atan, atan2) - no new dependencies
- **Phase 5**: Exponential base functions (exp, log, exp2, log2) - needed by hyperbolic and pow
- **Phase 6**: Hyperbolic trig (sinh, cosh, tanh, asinh, acosh, atanh) - depends on exp, log, sqrt
- **Phase 7**: Power function (pow) - depends on exp2, log2

## Questions

1. **Reference Implementation**: ✅ **DECIDED** - Use multiple references:
   - **libfixmath** (primary): sin, cos, tan, asin, acos, atan, atan2, sqrt, exp, log, log2
   - **fr_math**: exp2 (pow2) - can use as reference for exp2 implementation
   - **fpm**: pow - has pow implementation using exp2/log2: `pow(x,y) = exp2(log2(x) * y)`
   - **Mathematical formulas** (for hyperbolic): 
     - sinh(x) = (exp(x) - exp(-x)) / 2
     - cosh(x) = (exp(x) + exp(-x)) / 2  
     - tanh(x) = sinh(x) / cosh(x)
     - asinh(x) = log(x + sqrt(x² + 1))
     - acosh(x) = log(x + sqrt(x² - 1)) for x >= 1
     - atanh(x) = (1/2) * log((1+x)/(1-x)) for |x| < 1
   - **Decision**: Use libfixmath where available, fr_math for exp2 reference, fpm for pow reference, derive hyperbolic from mathematical formulas using exp/log/sqrt

2. **Implementation Order**: ✅ **DECIDED** - Reorder to respect dependencies:
   - **Phase 3**: Tangent (tan) - depends on sin/cos, implement first
   - **Phase 4**: Inverse trig (asin, acos, atan, atan2) - no new dependencies, can use existing sqrt
   - **Phase 5**: Exponential base (exp, log, exp2, log2) - needed by hyperbolic and pow
   - **Phase 6**: Hyperbolic trig (sinh, cosh, tanh, asinh, acosh, atanh) - needs exp, log, sqrt
   - **Phase 7**: Power (pow) - needs exp2, log2
   - **Decision**: Reordered phases respect dependencies, tan implemented first since it's simplest

3. **Function Dependencies**: ✅ **DECIDED** - Use relationships where appropriate:
   - `tan` = `sin(x) / cos(x)` (libfixmath does this)
   - `acos` = `π/2 - asin(x)` (libfixmath does this)
   - `atan` = `atan2(x, 1)` (libfixmath does this)
   - `cosh` = `(exp(x) + exp(-x)) / 2` (mathematical formula)
   - `tanh` = `sinh(x) / cosh(x)` (mathematical formula)
   - **Decision**: Represent functions in terms of each other where the reference library does, or where mathematical relationships are straightforward

4. **Two-Argument Functions**: ✅ **DECIDED** - Extend current pattern to support 2-arg functions:
   - Extend `map_testcase_to_builtin()` to return both BuiltinId and expected argument count
   - Modify `convert_call()` to check expected argument count (1 or 2)
   - Extract and map the correct number of arguments
   - Create builtin call with appropriate number of arguments
   - **Decision**: Extend the current 1-arg pattern to handle 2-arg functions (atan2, pow) in the same conversion logic

5. **Test Tolerances**: ✅ **DECIDED** - Start with 0.01, adjust per function, use reference test cases:
   - Start with 0.01 (1%) tolerance for all functions
   - Adjust per function based on accuracy if tests fail
   - **Test cases**: Take test cases from reference implementations (libfixmath, fr_math, fpm) to ensure comprehensive testing
   - Trust reference implementations' test coverage
   - **Decision**: Use 0.01 tolerance initially, adjust as needed; source test cases from reference libraries

6. **Acceptance Criteria**: ✅ **DECIDED** - Each phase has its own acceptance criteria:
   - Phase 3: `builtins/phases/02-basic-trig.glsl` passes (tan test)
   - Phase 4: `builtins/phases/03-inverse-trig.glsl` passes
   - Phase 5: `builtins/phases/05-exponential.glsl` passes (exp, log, exp2, log2 tests)
   - Phase 6: `builtins/phases/04-hyperbolic-trig.glsl` passes
   - Phase 7: `builtins/phases/05-exponential.glsl` passes (pow tests)
   - **Decision**: Each phase has its own acceptance criteria

