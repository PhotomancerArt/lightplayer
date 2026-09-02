layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;

// Deliberately faults every frame: the loop never ends, the per-pixel fuel
// tank drains, and the runtime traps — a RUNTIME failure of a shader that
// compiled fine. The engine reports the node as `Fault`, and every output
// of this project shows the fault pattern instead of black
// (docs/adr/2026-09-02-fault-is-never-black.md). Fuel metering is on for every backend
// (`lpvm-native` NativeOptions.fuel defaults true; the browser sim's
// `infinite_loop_shader_reports_fuel_error_and_keeps_ticking` pins it),
// so this never hangs or reboots a board.
vec4 render_2d(vec2 pos) {
    float acc = 0.0;
    while (true) { acc += 0.001; }
    return vec4(acc, 0.0, 0.0, 1.0);
}
