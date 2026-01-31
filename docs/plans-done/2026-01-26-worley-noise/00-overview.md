# Plan: Worley Noise Implementation

## Overview

Implement Worley noise (cellular noise) functions for the LP builtin library, following the same pattern as Simplex noise. Worley noise generates cellular patterns based on the distance to the nearest feature point in a grid.

The implementation includes:
- 2D Worley noise (`lpfx_worley2`, `lpfx_worley2_value`)
- 3D Worley noise (`lpfx_worley3`, `lpfx_worley3_value`)
- Integration with the existing builtin system (auto-registered via macro)
- Tests comparing against noise-rs reference implementation

## Phases

1. **Create worley module structure** - Create `worley/` directory and `mod.rs` file
2. **Implement 2D Worley distance** - Create `worley2_q32.rs` with distance function
3. **Implement 2D Worley value** - Create `worley2_value_q32.rs` with value function
4. **Implement 3D Worley distance** - Create `worley3_q32.rs` with distance function
5. **Implement 3D Worley value** - Create `worley3_value_q32.rs` with value function
6. **Regenerate builtin registry** - Run builtin generator to register new functions
7. **Add tests** - Create tests comparing against noise-rs reference implementation
8. **Update exports and documentation** - Update `mod.rs` and ensure documentation is complete

## Success Criteria

- All four Worley functions compile and are registered in the builtin system
- Functions can be called from GLSL with correct type checking
- Vector arguments are properly flattened and passed to internal functions
- Tests pass comparing against noise-rs reference implementation
- Output values are in approximately [-1, 1] range (Q32)
- Code is formatted and follows project conventions
