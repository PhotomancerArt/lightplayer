# Phase 19: Debug UI - Mapping Overlay

## Goal

Add mapping visualization overlaid on the texture in the debug UI.

## Tasks

1. Extend `src/debug_ui.rs`:
   - Add mapping overlay visualization
   - Draw fixture mappings as circles/points on the texture
   - Show mapping center points and radius
   - Color-code mappings by fixture or channel
2. Integrate into texture visualization:
   - Overlay mapping circles on texture image
   - Show mapping metadata (channel, center, radius)
   - Allow toggling mapping visibility
3. Handle multiple fixtures:
   - Display all fixtures that map to the texture
   - Different colors for different fixtures
   - Show which LED channels map to which points

## Success Criteria

- Mappings are overlaid on texture visualization
- Mapping circles/points are visible
- Multiple fixtures are distinguishable
- All code compiles without warnings

