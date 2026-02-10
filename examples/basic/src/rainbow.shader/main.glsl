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

    //return prsd_demo(scaledCoord, time);
    return fbm_demo(scaledCoord, time);
    //return worley_demo(scaledCoord, time);
}

vec4 worley_demo(vec2 scaledCoord, float time) {
    // Call built-in 3D Worley noise, returns vec2(d0, d1)
    float noiseValue = lpfx_worley(scaledCoord * 2, 0u) / 2 + 0.5;

    // Use the distance to the closest point for visualization
    float hue = cos(noiseValue * 3.1415 + time) / 2 + .5;

    vec3 rgb = lpfx_hsv2rgb(vec3(hue, 1.0, 1.0));
    return vec4(rgb, 1.0);
}

vec4 fbm_demo(vec2 scaledCoord, float time) {
    float noiseValue = lpfx_fbm(
        scaledCoord,
        3,
        0u
    );
    float hue = cos(noiseValue * 3.1415 + time) / 2 + .5;
    vec3 rgb = lpfx_hsv2rgb(vec3(mod(time * 0.1 + hue / 3.0, 1.0), 1.0, 1.0));

    return vec4(rgb, 1.0);
}

vec4 prsd_demo(vec2 scaledCoord, float time) {
    // Sample Periodic Simplex Rotational Domain noise
    // psrdnoise returns both noise value and gradient vector
    vec2 gradient;
    float noiseValue = lpfx_psrdnoise(
        scaledCoord, // Input coordinates
        vec2(0.0), // Period (0.0 = no tiling, or use vec2(10.0) for tiling)
        time, // Rotation angle (alpha) - animate with time
        gradient       // Output gradient vector (out parameter)
    );

    // Use gradient to add detail:
    // 1. Gradient magnitude adds texture variation
    float gradientMag = length(gradient);
    float textureDetail = 0.3 + 0.2 * smoothstep(0.0, 5.0, gradientMag);

    // 2. Combine noise value with gradient influence
    float hue = cos(noiseValue * 3.1415 + time) / 2 + .5;

    // 3. Use gradient angle for saturation (normalized to [0, 1], minimum 0.25)
    // atan returns [-π, π], normalize to [0, 1] by adding 0.5 after dividing by 2π
    float gradientAngle = atan(gradient.y, gradient.x) / (2.0 * 3.14159) + 0.5;
    // Map to [0.25, 1.0] range: scale [0, 1] to [0.25, 1.0]
    float saturation = 0.25 + 0.75 * gradientAngle;

    // Convert HSV to RGB with gradient-enhanced detail
    vec3 rgb = lpfx_hsv2rgb(vec3(
                            mod(time * 0.1 + hue / 3.0, 1.0),
                            1.0,
                            gradientAngle));

    // Clamp to [0, 1] and return
    // return vec4(hue, gradient.x, gradient.y, 1.0);
    return vec4(rgb, 1.0);
}