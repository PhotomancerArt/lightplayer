# Plan: Q32 Metrics App

## Overview

Create a new app `q32-metrics` in `lp-glsl/apps/` that tracks the effects of q32 transform optimizations by generating detailed before/after reports for GLSL test files. The app compiles GLSL files, applies the q32 transform, collects statistics, and generates TOML reports with per-function CLIF IR files.

## Phases

1. Create app structure and dependencies
2. Implement CLI argument parsing
3. Implement GLSL compilation and transform logic
4. Implement statistics collection
5. Implement CLIF file writing utilities
6. Implement report generation (TOML)
7. Create wrapper script
8. Add initial test GLSL files
9. Testing and cleanup

## Success Criteria

- App successfully compiles GLSL files and applies q32 transform
- Statistics are collected accurately (blocks, instructions, values, CLIF size)
- Per-function CLIF files are generated correctly
- TOML reports are generated with proper structure
- Wrapper script runs with good defaults
- Initial test GLSL files are included
- Code compiles without errors
- All warnings addressed
- Code formatted with `cargo +nightly fmt`
