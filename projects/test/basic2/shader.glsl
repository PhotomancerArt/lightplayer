const float TAU = 6.2831853;

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float panPhase;
layout(binding = 2) uniform float scalePhase;
layout(binding = 3) uniform float huePhase;

// Naga GLSL-in resolves calls in source order; define helpers before render().
vec4 worley_demo(vec2 scaledCoord, float huePhase) {
    // Call built-in 3D Worley noise, returns vec2(d0, d1)
    float noiseValue = lpfn_worley(scaledCoord * 2, 0u) / 2 + 0.5;

    // Use the distance to the closest point for visualization
    float hue = cos(noiseValue * 3.1415 + TAU * huePhase) / 2 + .5;

    vec3 rgb = lpfn_hsv2rgb(vec3(hue, 1.0, 1.0));
    return vec4(rgb, 1.0);
}

vec4 render_2d(vec2 pos) {
    // Pan through noise using three phasors with oscillation to stay bounded.
    // Oscillate between minZoom and maxZoom to avoid unbounded growth.
    // sin returns [-1, 1], map to [0, 1] then use mix for interpolation.
    float pan = mix(1.0, 8.0, 0.5 * (sin(TAU * panPhase) + 1.0));

    float scale = mix(.04, .06, 0.5 * (sin(TAU * scalePhase) + 1.0));

    // Scale from center: translate to center, scale, translate back
    vec2 center = outputSize * 0.5;
    vec2 dir = pos - center;
    vec2 scaledCoord = center + dir * scale;

    return worley_demo(scaledCoord, huePhase);
}
