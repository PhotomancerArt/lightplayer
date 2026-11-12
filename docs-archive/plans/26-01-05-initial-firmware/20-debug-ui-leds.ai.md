# Phase 20: Debug UI - LED Visualization Enhancement

## Goal

Enhance the LED visualization in the debug UI to show more information and better layout.

## Tasks

1. Enhance `src/led_output.rs` rendering:
   - Improve LED circle rendering (better sizing, spacing)
   - Add LED index labels
   - Show color values (RGB) for each LED
   - Add zoom/pan controls for large LED counts
2. Integrate with debug UI:
   - Add LED panel/tab to egui
   - Show LED state synchronized with texture/mapping
   - Display LED metadata (channel, mapped texture coordinates)
3. Add interactivity:
   - Click on LED to highlight corresponding mapping point
   - Show tooltip with LED details
   - Sync selection between LED view and texture view

## Success Criteria

- LED visualization is enhanced with labels and metadata
- LEDs are synchronized with texture/mapping view
- Interactive features work (click, highlight, tooltip)
- All code compiles without warnings

