# Phase 7: Add UI status indicators

## Description

Add visual status indicators (colored circles) next to each node name in the debug UI to show node status at a glance. Green for `Ok`, red for `Error` or `InitError`, yellow for `Warn`, gray for `Created`.

## Changes

### File: `lp-cli/src/debug_ui/panels.rs`

1. **Modify `render_all_nodes_panel()` function**:
   - Add status indicator circle next to each node name/checkbox
   - Use `egui::painter` to draw circles or `egui::widgets::Label` with colored text
   - Circle should be small (8-10 pixels) and positioned next to the node name
   - Color mapping:
     - Green circle for `Ok`
     - Red circle for `Error` or `InitError`
     - Yellow circle for `Warn`
     - Gray circle for `Created`

2. **Status indicator implementation**:
   - Use `ui.painter().circle_filled()` to draw filled circles
   - Or use `ui.label()` with colored text/emoji
   - Position indicator before or after the checkbox/node name

### File: `lp-cli/src/debug_ui/nodes/shader.rs`

1. **Enhance error display** (optional):
   - Make error text more prominent if needed
   - Could add warning icon or make error text larger/bolder
   - Current implementation already shows error in red, which may be sufficient

## Success Criteria

- Status indicators are visible next to each node name
- Colors correctly indicate node status
- Indicators are small and don't clutter the UI
- Code compiles without errors
- UI renders correctly

## Notes

- Status indicators provide quick visual feedback
- Users can see node status without expanding node details
- Implementation can use circles or colored text/emoji depending on what looks best
