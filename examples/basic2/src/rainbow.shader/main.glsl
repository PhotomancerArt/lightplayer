vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    // Pan through noise using time with oscillation to stay bounded
    // Oscillate between minZoom and maxZoom to avoid unbounded growth
    float panSpeed = .3;
    // Use sine to oscillate between min and max zoom
    // sin returns [-1, 1], map to [0, 1] then use mix for interpolation
    float pan = mix(1.0, 8.0, 0.5 * (sin(time * panSpeed) + 1.0));

    float scaleSpeed = .7;
    float scale = mix(.04, .06, 0.5 * (sin(time * scaleSpeed) + 1.0));

    // Scale from center: translate to center, scale, translate back
    vec2 center = outputSize * 0.5;
    vec2 dir = fragCoord - center;
    vec2 scaledCoord = center + dir * scale;

    return worley_demo(scaledCoord, time);
}

vec4 worley_demo(vec2 scaledCoord, float time) {
    // Call built-in 3D Worley noise, returns vec2(d0, d1)
    float noiseValue = lpfx_worley(scaledCoord * 2, 0u) / 2 + 0.5;

    // Use the distance to the closest point for visualization
    float hue = cos(noiseValue * 3.1415 + time) / 2 + .5;

    vec3 rgb = lpfx_hsv2rgb(vec3(hue, 1.0, 1.0));
    return vec4(rgb, 1.0);
}