# Phase 18: Debug UI - Texture Visualization

## Goal

Add a debug UI panel to visualize textures in the egui window.

## Tasks

1. Create `src/debug_ui.rs` module:
   - Add texture visualization function
   - Display texture as an image in egui
   - Show texture metadata (size, format)
2. Integrate into main UI:
   - Add texture panel/tab to egui
   - Display all textures from the project
   - Show texture data (for 64x64 RGB8, render as 64x64 image)
3. Handle different texture formats:
   - RGB8: 3 bytes per pixel
   - RGBA8: 4 bytes per pixel  
   - R8: 1 byte per pixel (grayscale)
4. Convert texture data to egui image format

## Success Criteria

- Textures are displayed in the debug UI
- Texture metadata is shown (size, format)
- Different formats render correctly
- All code compiles without warnings

