# Phase 6: Update render_fixtures_panel() to use render_fixture()

## Goal

Update `render_fixtures_panel()` to iterate over fixtures, get configs and runtimes, and call `render_fixture()`.

## Tasks

1. Update `render_fixtures_panel()` to:
   - Get fixture IDs from runtime
   - For each fixture ID:
     - Get fixture runtime and config
     - Call `render_fixture()` with fixture_id, config, project, and runtime
2. Remove placeholder message code

## Success Criteria

- Fixtures panel displays fixture details with texture previews and mapping overlays
- Uses existing `render_fixture()` helper function
- Code compiles without errors

