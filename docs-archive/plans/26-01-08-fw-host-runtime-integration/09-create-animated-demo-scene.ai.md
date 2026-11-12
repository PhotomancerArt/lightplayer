# Phase 9: Create animated demo scene

## Goal

Update default project to have an animated rotating color wheel shader.

## Tasks

1. Update `create_default_project()` in `fw-host/src/app.rs`:
   - Replace static gray shader with rotating color wheel:
     - Use `time` parameter to rotate hue
     - Calculate angle from position (fragCoord)
     - Combine with time for rotation
     - Convert HSV to RGB
   - Example shader:
     ```glsl
     vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
         vec2 center = outputSize * 0.5;
         vec2 dir = fragCoord - center;
         float angle = atan(dir.y, dir.x) + time;
         float hue = (angle / (2.0 * 3.14159) + 1.0) * 0.5;
         // Convert HSV to RGB (simplified)
         return vec4(hue, 0.8, 0.9, 1.0);
     }
     ```

2. Ensure shader compiles and runs correctly

3. Verify animation is visible in LED output

## Success Criteria

- Default project has animated shader
- Shader compiles without errors
- Animation is visible (LEDs change over time)
- Code compiles without warnings

