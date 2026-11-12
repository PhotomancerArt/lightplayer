# Plan: Fixture Path Mapping Support

## Overview

Implement support for structured path-based mapping configurations for fixtures, starting with circular displays (RingArray). This includes generating LED positions from path specifications, handling texture resolution changes, and standardizing coordinate space to [0, 1] across the fixture system.

## Phases

1. Update coordinate space to [0, 1] - Update `MappingPoint`, comments, and render code to use texture space [0, 1] instead of [-1, 1]
2. Implement RingArray path generation - Add functions to generate `MappingPoint` positions from `RingArray` configuration
3. Add comprehensive tests for RingArray generation - Test single/multiple rings, ordering, angles, channel assignment, edge cases
4. Add texture resolution change detection - Track texture dimensions and regenerate mappings when resolution changes
5. Remove string-based mapping support - Remove "linear" string mapping, update runtime to use `MappingConfig` enum
6. Update builder and example JSON - Update `FixtureBuilder` and example fixture config to use new `MappingConfig` format
7. Cleanup and finalization - Fix warnings, update all comments, ensure tests pass, format code

## Success Criteria

- All fixture coordinates use texture space [0, 1]
- RingArray path generation works correctly for all configurations
- Comprehensive test coverage for RingArray generation
- Mappings regenerate when texture resolution changes
- String-based mapping support removed
- Builder and examples use new `MappingConfig` format
- All code compiles without warnings
- All tests pass
- Code formatted with `cargo +nightly fmt`
