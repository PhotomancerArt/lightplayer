# Phase 4: Update UI to use resolved handles

## Goal

Update fixture panel to use `texture_handle` from `FixtureState` instead of finding any texture node.

## Tasks

1. Update `lp-app/apps/lp-cli/src/debug_ui/panels.rs`:
   - In `render_fixture_panel()`:
     - Check `state.texture_handle` instead of finding any texture
     - If handle is `Some`, find that node in the view
     - If handle is `None`, show "No texture available" message
     - Remove TODO comment about extracting texture reference

2. Ensure proper error handling:
   - Handle case where texture handle exists but node not found in view
   - Handle case where texture handle is `None` (fixture not initialized)
   - Show appropriate error messages

## Success Criteria

- Fixture panel shows correct texture (the one referenced by fixture)
- Panel handles missing texture gracefully
- No more finding "any texture" workaround
- Code compiles without errors
- UI displays correctly

## Implementation Notes

- Use `view.nodes.get(&texture_handle)` to find texture node
- Verify node is actually a texture node before displaying
- Show helpful error messages when texture unavailable
- This completes the fixture texture reference fix
