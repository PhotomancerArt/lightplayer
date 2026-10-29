// Each lamp painted by where it is: distinct lamps, distinct colours, never
// dark (blue is always half). What the placement tests read lamps by.
layout(binding = 0) uniform vec2 outputSize;

vec4 render_2d(vec2 pos) {
    vec2 uv = pos / outputSize;
    return vec4(uv.x, uv.y, 0.5, 1.0);
}
