# Plan: Dev UI for lp-cli Client

## Overview

Implement a debug UI for the lp-cli dev command that displays project node state from the client perspective. The UI will show all nodes with detail tracking checkboxes, render node-specific detail panels, and sync state from the server using the client-server protocol.

This demonstrates that the client-server sync works correctly and provides a debugging interface similar to the old debug app, but using the client view instead of direct runtime access.

## Phases

1. Extend TextureState with width, height, format
2. Extend FixtureState with MappingCell and mapping_cells
3. Update engine runtime state extraction
4. Add project_sync method to AsyncLpClient
5. Create debug_ui module structure
6. Implement UI state and sync logic
7. Implement panel rendering functions
8. Integrate UI into dev command
9. Add --headless flag
10. Cleanup and finalization

## Success Criteria

- UI displays all nodes with checkboxes for detail tracking
- Individual node checkboxes toggle detail tracking
- "All detail" checkbox controls all nodes
- Texture nodes display image with correct format
- Shader nodes display GLSL code from state
- Fixture nodes display texture with mapping overlay
- Output nodes display channel data
- Sync happens automatically when not in progress
- No more than one GetChanges request in flight
- Code compiles without warnings
- All tests pass
