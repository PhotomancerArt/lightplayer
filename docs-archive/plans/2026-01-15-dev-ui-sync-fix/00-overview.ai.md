# Plan: Dev UI Sync Fix

## Overview

Fix three critical issues preventing the debug UI from functioning:
1. Transport sharing between project loader and UI
2. Sync mechanism implementation  
3. Fixture texture reference resolution

## Phases

1. **Refactor AsyncLpClient for shared transport**: Change `AsyncLpClient` to accept `Arc<Mutex<Transport>>` instead of `Box<Transport>`
2. **Add resolved handles to FixtureState**: Add `texture_handle` and `output_handle` fields to `FixtureState` and extract them from runtime
3. **Implement sync mechanism**: Restructure `project_sync()` and `handle_sync()` to actually perform syncs without holding locks across await
4. **Update UI to use resolved handles**: Update fixture panel to use `texture_handle` from state instead of finding any texture
5. **Cleanup and finalization**: Fix warnings, ensure code is clean and tested

## Success Criteria

- UI displays project state from server
- Sync happens automatically in UI update loop
- Fixture panel shows correct texture (not just any texture)
- Transport is shared between loader and UI
- Code compiles without warnings
- No deadlocks or race conditions
