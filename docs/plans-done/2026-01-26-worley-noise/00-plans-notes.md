# Questions for Worley Noise Implementation Plan

## Context

We want to add Worley noise (cellular noise) functions to the LP builtin library, following the same pattern as the Simplex noise implementation. Worley noise is a cellular noise algorithm that generates patterns based on the distance to the nearest feature point in a grid.

The reference implementation is in `/Users/yona/dev/photomancer/oss/noise-rs/src/core/worley.rs` and shows:
- 2D, 3D, and 4D implementations
- Multiple distance functions: euclidean, euclidean_squared, manhattan, chebyshev
- Two return types: Distance (returns distance to nearest point) and Value (returns hash value of nearest cell)

## Questions

### Q1: Which Dimensions to Implement

**Context**: The reference implementation has 2D, 3D, and 4D versions. Simplex noise was implemented for 1D, 2D, and 3D.

**Question**: Which dimensions should we implement for Worley noise?
- Option A: 2D and 3D only (matches common use cases)
- Option B: 2D, 3D, and 4D (complete set like reference)
- Option C: Just 2D initially (start simple)

**Suggested Answer**: Option A - 2D and 3D. These are the most commonly used dimensions. 4D can be added later if needed. 1D Worley noise is less useful than 1D Simplex.

**ANSWERED**: Option A - 2D and 3D only. 4D can be added later if needed.

### Q2: Distance Function Support

**Context**: The reference implementation supports four distance functions:
- Euclidean (standard distance)
- Euclidean squared (faster, no sqrt)
- Manhattan (L1 distance)
- Chebyshev (L∞ distance)

**Question**: Which distance functions should we support?
- Option A: Just euclidean (simplest, most common)
- Option B: Euclidean and euclidean_squared (covers most cases, squared avoids sqrt)
- Option C: All four distance functions (complete set)

**Suggested Answer**: Option B - Euclidean and euclidean_squared. Euclidean squared is faster (no sqrt) and often preferred for performance. Manhattan and Chebyshev are less commonly used and can be added later if needed.

**ANSWERED**: Just euclidean_squared. Simpler API, faster (no sqrt), and sufficient for most use cases. If users need actual distance, they can take sqrt in GLSL. This reduces the function count significantly.

### Q3: Return Type Handling

**Context**: Worley noise can return either:
- Distance: The distance to the nearest feature point
- Value: A hash value (0-1) based on the nearest cell's hash

**Question**: How should we handle return types?
- Option A: Two separate functions per dimension (e.g., `lpfx_worley2_distance`, `lpfx_worley2_value`)
- Option B: Single function with a parameter to select return type
- Option C: Just implement Distance return type (most common use case)

**Suggested Answer**: Option A - Two separate functions per dimension. This matches GLSL's function overloading pattern and is clearer than a parameter. Users can call the appropriate function based on their needs.

### Q4: Function Naming Convention

**Context**: Simplex noise uses `lpfx_snoise1`, `lpfx_snoise2`, `lpfx_snoise3`. Worley noise needs to distinguish distance vs value.

**Question**: What naming convention should we use?
- Option A: `lpfx_worley2_distance`, `lpfx_worley2_value`, `lpfx_worley3_distance`, `lpfx_worley3_value`
- Option B: `lpfx_worley2d`, `lpfx_worley2v`, `lpfx_worley3d`, `lpfx_worley3v` (shorter)
- Option C: `lpfx_worley2_dist`, `lpfx_worley2_val`, `lpfx_worley3_dist`, `lpfx_worley3_val` (abbreviated)

**Suggested Answer**: Option A - Full names are clearer and more readable. Matches the pattern of being explicit about what the function does.

**UPDATED**: Base name for distance, `_value` suffix for value - matches lygia convention. `lpfx_worley2` returns distance, `lpfx_worley2_value` returns hash value.

**ANSWERED**: Base name for distance, `_value` suffix for value - `lpfx_worley2`, `lpfx_worley2_value`, `lpfx_worley3`, `lpfx_worley3_value`. This matches lygia's convention where the base function returns distance.

### Q5: Distance Function Parameter

**Context**: The reference implementation takes a distance function as a closure/function pointer. In GLSL, we can't pass functions as parameters.

**Question**: How should we handle distance function selection?
- Option A: Separate functions for each distance function (e.g., `lpfx_worley2_euclidean_distance`, `lpfx_worley2_euclidean_squared_distance`)
- Option B: Use a numeric parameter (0=euclidean, 1=euclidean_squared) - but this is less type-safe
- Option C: Just implement euclidean_squared (fastest, most common) and skip the parameter

**Suggested Answer**: Option A - Separate functions for each distance function. This is type-safe, clear, and matches GLSL's lack of function pointers. If we only implement euclidean and euclidean_squared, we'd have: `lpfx_worley2_euclidean_distance`, `lpfx_worley2_euclidean_value`, `lpfx_worley2_euclidean_squared_distance`, `lpfx_worley2_euclidean_squared_value`, etc.

**UPDATED**: Since we're only implementing euclidean_squared, the function names can be simplified. We can drop the distance function name from the function name since there's only one option. This gives us: `lpfx_worley2_distance`, `lpfx_worley2_value`, `lpfx_worley3_distance`, `lpfx_worley3_value`.

### Q6: Initial Implementation Scope

**Context**: Given the combinations (dimensions × distance functions × return types), we could have many functions.

**Question**: What should be the initial implementation scope?
- Option A: 2D euclidean_squared distance and value (4 functions: 2D × 1 distance × 2 return types)
- Option B: 2D and 3D euclidean_squared (8 functions: 2 dimensions × 1 distance × 2 return types)
- Option C: 2D and 3D with both euclidean and euclidean_squared (16 functions: 2 dimensions × 2 distances × 2 return types)

**Suggested Answer**: Option B - 2D and 3D with euclidean_squared. This gives a complete 2D/3D set with the fastest distance function. Euclidean can be added later if needed. This results in 4 functions: `lpfx_worley2_euclidean_squared_distance`, `lpfx_worley2_euclidean_squared_value`, `lpfx_worley3_euclidean_squared_distance`, `lpfx_worley3_euclidean_squared_value`.

**UPDATED**: With just euclidean_squared, we have 4 functions: `lpfx_worley2` (distance), `lpfx_worley2_value`, `lpfx_worley3` (distance), `lpfx_worley3_value`. Base name returns distance (primary use case), `_value` suffix returns hash value.

### Q7: Return Value Range

**Context**: Simplex noise returns values in approximately [-1, 1] range. The reference Worley implementation also scales to [-1, 1] (`value * 2.0 - 1.0`). However, for graphics work, [0, 1] is often more convenient since colors and many operations expect [0, 1].

**Question**: What range should Worley noise return?
- Option A: [-1, 1] (matches Simplex, matches reference implementation, symmetric)
- Option B: [0, 1] (more convenient for graphics, easier to use directly)

**Tradeoffs**:
- [-1, 1]: Symmetric, easy to convert to [0, 1] with `* 0.5 + 0.5`, matches Simplex convention
- [0, 1]: More convenient for graphics, can be used directly without normalization, but inconsistent with Simplex

**Note**: Changing Simplex to [0, 1] would be a breaking change. We could:
- Keep Worley at [-1, 1] to match Simplex (consistency)
- Use [0, 1] for Worley (better UX, but inconsistent with Simplex)
- Document the difference clearly if we choose different ranges

**Suggested Answer**: Option A - [-1, 1] to match Simplex. Consistency is valuable, and conversion is simple (`* 0.5 + 0.5`). However, if graphics convenience is prioritized, Option B is reasonable.

**ANSWERED**: Option A - [-1, 1] to match Simplex. Consistency is valuable, and it also matches lygia (which may be considered in the future). Conversion to [0, 1] is simple if needed (`* 0.5 + 0.5`).
