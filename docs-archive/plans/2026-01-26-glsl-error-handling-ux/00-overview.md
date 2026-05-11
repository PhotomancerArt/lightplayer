# Plan: GLSL Error Handling UX Improvements

## Overview

Improve the UX around GLSL error handling in `lp-cli dev` to make development smoother:
- Projects should start even if shader nodes have GLSL compilation errors
- File changes should be processed even when there's a compilation error
- GLSL errors should be prominently displayed in the debug UI
- Visual status indicators should show node status at a glance
- Status changes should be logged to console for easy CLI usage

## Phases

1. Update ShaderRuntime to not fail on compilation errors
2. Update ProjectRuntime to handle compilation errors gracefully
3. Update ensure_all_nodes_initialized to not fail on Error status
4. Add status synchronization in get_changes
5. Add client-side status change logging
6. Add UI status indicators
7. Cleanup and finalization

## Success Criteria

- Projects start successfully even if shader nodes have GLSL compilation errors
- File changes are processed even when there's a compilation error
- GLSL errors are prominently displayed in the debug UI
- Status indicators (colored circles) are visible next to each node name
- Status changes are logged to console with clear messages
- All code compiles without errors
- All tests pass
