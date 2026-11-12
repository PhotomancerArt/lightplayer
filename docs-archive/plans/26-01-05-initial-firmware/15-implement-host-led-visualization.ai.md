# Phase 15: Implement host LED visualization (egui)

## Goal

Implement LED visualization on host using egui.

## Tasks

1. Create `src/led_output.rs`:
   - Implement `lp-core::traits::LedOutput` trait for host
   - Store pixel data in memory
   - Render pixels as circles in egui window
   - Handle different LED counts and layouts
2. Integrate visualization into `main.rs`:
   - Add LED visualization panel to egui UI
   - Update visualization when pixels are written
   - Show LED positions and colors
3. Test visualization with sample data

## Success Criteria

- LED output trait implementation works
- LEDs are visualized as circles in egui window
- Visualization updates correctly
- All code compiles without warnings

