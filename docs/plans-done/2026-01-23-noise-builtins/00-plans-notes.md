# Questions for Noise Builtins Plan

## Context

We want to add Lightplayer-specific builtin functions for noise generation that can be called from
GLSL shaders. These functions will:

- Use the `lp_` prefix (e.g., `lp_perlin3`, `lpfx_hash`)
- Support Q32 fixed-point arithmetic (the only mode currently supported)
- Take vector types as arguments (vec2, vec3, ivec2, ivec3)
- Include frequency and seed parameters
- Be callable from GLSL like normal functions

## Questions

### Q1: Function Naming Convention

**Context**: Current builtins use `__lp_q32_*` naming internally, but these are implementation
details. We need user-facing names like `lp_perlin3`.

**Question**: Should we use:

- Option A: `lp_perlin3` (simple, user-facing name)
- Option B: `lp_q32_perlin3` (explicit about fixed-point)
- Option C: Something else?

**Suggested Answer**: Option A (`lp_perlin3`) - simpler and cleaner. The fixed-point nature is
implicit since that's the only mode.

**ANSWERED**: Option B (`lp_q32_perlin3`) - explicit about fixed-point format, clearer and more
consistent with internal naming.

**UPDATED**: User wants `lp_` prefix for clarity to match usage in code. So user-facing names:

- `lpfx_hash` (not `lp_q32_hash`)
- `lpfx_snoise1` (not `lp_q32_simplex1`)
- `lpfx_snoise2` (not `lp_q32_simplex2`)
- `lpfx_snoise3` (not `lp_q32_simplex3`)

Internal implementation functions can still use `__lp_q32_*` naming.

### Q2: Builtin Category vs User Functions

**Context**: The compiler has:

- GLSL builtins (checked via `is_builtin_function()`)
- User-defined functions (looked up in function registry)

**Question**: Should `lp_*` functions be:

- Option A: A new category checked separately (like "lp_glsl_builtins")
- Option B: Registered as user functions but with special handling
- Option C: Added to the GLSL builtin system but with `lp_` prefix

**Suggested Answer**: Option A - Create a new category. Check for `lp_` prefix after GLSL builtins
but before user functions. This keeps them separate and allows special handling.

**ANSWERED**: Option A - New category called "LP Library Functions" (LpLibFn). These are
Lightplayer's standard library functions for shaders, distinct from:

- GLSL builtins (standard GLSL functions)
- Internal builtins (`__lp_q32_*` implementation functions)
- User-defined functions

Naming:

- Category: `LpLibFn` / `LpLibraryFunction`
- Registry enum: `LpLibFnId`
- Check function: `is_lp_lib_fn()` (checks for `lp_` prefix)
- User-facing names: `lpfx_hash`, `lpfx_snoise1`, `lpfx_snoise2`, `lpfx_snoise3`

### Q3: Vector Argument Handling

**Context**: Vectors are passed as flattened components (vec2 = 2 i32s, vec3 = 3 i32s). Builtins
currently take scalar types.

**Question**: How should we handle vector arguments?

- Option A: Flatten vectors to individual i32 parameters (vec3 → 3 i32 params)
- Option B: Pass vectors as pointers/structs
- Option C: Create separate overloads for each dimension (lp_perlin1, lp_perlin2, lp_perlin3)

**Suggested Answer**: Option A - Flatten vectors to individual i32 parameters. This matches how the
compiler currently handles vectors and is simplest. The function signature will be
`lp_perlin3(i32 x, i32 y, i32 z, i32 frequency, u32 seed) -> i32`.

**ANSWERED**: Option A - Flatten vectors to individual parameters. Function signatures (updated
after removing frequency and using lp_ prefix):

- `lpfx_snoise1(i32 x, u32 seed) -> i32`
- `lpfx_snoise2(i32 x, i32 y, u32 seed) -> i32`
- `lpfx_snoise3(i32 x, i32 y, i32 z, u32 seed) -> i32`
- `lpfx_hash(u32 x) -> u32`
- `lpfx_hash(u32 x, u32 y) -> u32`
- `lpfx_hash(u32 x, u32 y, u32 z) -> u32`

### Q4: Function Signature Generation

**Context**: The builtin registry auto-generates signatures from function definitions. Current
builtins are simple (i32 → i32).

**Question**: How should we generate signatures for functions with vector arguments?

- Option A: Manually specify signatures in the registry
- Option B: Extend the auto-generator to understand vector types
- Option C: Use a different registration mechanism for lp_* functions

**Suggested Answer**: Option B - Extend the auto-generator, but for initial implementation, Option
A (manual) might be simpler. We can start with manual and refactor later.

**ANSWERED**: Option A - Manual signature specification for now. Simpler and faster to implement.
Can refactor to auto-generation later if needed.

### Q5: Hash Function Implementation

**Context**: We need a hash function for noise generation. The noiz library uses a custom hash
optimized for noise.

**Question**: Should we:

- Option A: Implement the exact noiz hash algorithm
- Option B: Use a simpler hash (like xxhash or murmur)
- Option C: Create our own custom hash

**Suggested Answer**: Option A - Use the noiz hash algorithm. It's proven for noise generation and
we can reference their implementation.

**ANSWERED**: Option A - Use the noiz hash algorithm. Include credit/attribution in the code
comments. The algorithm uses bit rotations, XOR, and multiplication by prime 249,222,277, inspired
by https://nullprogram.com/blog/2018/07/31/

### Q6: Perlin Noise Algorithm Details

**Context**: Perlin noise needs:

- Hash function for gradient selection
- Gradient vectors (12 directions for 3D)
- Smooth interpolation (quintic curve)
- Scaling to [-1, 1] range

**Question**: Should we:

- Option A: Implement classic Perlin noise (as in noise-rs)
- Option B: Implement improved Perlin noise (with better gradients)
- Option C: Use Simplex noise instead (better quality, more complex)

**Suggested Answer**: Option A - Classic Perlin noise. It's standard, well-understood, and easier to
implement. We can add Simplex later if needed.

**ANSWERED**: Option C - Simplex noise. Better quality (less directional artifacts), faster in 3D (
interpolates 4 corners vs 8), and more isotropic. More complex to implement (requires skew/unskew
math) but worth it for quality.

### Q7: Frequency Parameter Handling

**Context**: Frequency scales the input coordinates. In Q32, frequency is also a Q32 value.

**Question**: How should frequency be applied?

- Option A: Multiply input coordinates by frequency before hashing
- Option B: Apply frequency in the hash function itself
- Option C: Pre-scale coordinates in the caller

**Suggested Answer**: Option A - Multiply input coordinates by frequency before hashing. This is
standard and matches how noise libraries work.

**ANSWERED**: Remove frequency parameter entirely. Caller can scale coordinates themselves using
fixed-point multiplication. Simpler API:

- `lp_q32_simplex1(i32 x, u32 seed) -> i32`
- `lp_q32_simplex2(i32 x, i32 y, u32 seed) -> i32`
- `lp_q32_simplex3(i32 x, i32 y, i32 z, u32 seed) -> i32`

### Q8: Seed Parameter Usage

**Context**: Seed affects the hash function output. In noiz, seed is XORed into the hash.

**Question**: How should seed be incorporated?

- Option A: XOR seed into hash input (like noiz)
- Option B: Use seed to offset hash table lookup
- Option C: Add seed to coordinates before hashing

**Suggested Answer**: Option A - XOR seed into hash input. This matches noiz and is simple to
implement.

**ANSWERED**: Option A - XOR seed into hash input. Incorporate seed during hash computation,
matching noiz's approach.

### Q9: Test Strategy

**Context**: We need to test noise functions produce correct output.

**Question**: How should we test?

- Option A: Compare against reference implementations (noise-rs, noiz)
- Option B: Test specific known outputs for given inputs
- Option C: Test properties (range, continuity, etc.)

**Suggested Answer**: Option A + C - Compare against reference implementations for correctness, and
test properties (output range, continuity) for quality.

**ANSWERED**: Test against noise-rs as reference implementation. Add noise-rs as a test-only
dependency. Compare our Q32 fixed-point implementations against noise-rs's f64 implementations by
converting between formats. Also test properties (output range [-1, 1], continuity, etc.) for
quality validation.

### Q10: Initial Function Set

**Context**: We want to start simple but future-proof.

**Question**: Which functions should we implement initially?

- Option A: Just hash and perlin3
- Option B: hash + perlin1 + perlin2 + perlin3
- Option C: hash + all perlin variants + fBm

**Suggested Answer**: Option B - hash + perlin1 + perlin2 + perlin3. This gives a complete set while
staying focused. fBm can come later.

**ANSWERED**: Option B - hash + simplex1 + simplex2 + simplex3. Complete set while staying focused.
fBm can be implemented by users in GLSL using the base functions, or added as a builtin later if
needed.

## Notes

### Enum Naming Convention

Enum variants in `LpLibFnId` should follow the function names with `Lp` prefix:

- `LpHash1`, `LpHash2`, `LpHash3`
- `LpSimplex1`, `LpSimplex2`, `LpSimplex3`
