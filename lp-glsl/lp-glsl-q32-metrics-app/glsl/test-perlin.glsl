// Hash function for pseudo-random gradient vectors
// Adapted for fixed-point arithmetic (clamped at 2^16)
float hash(vec2 p) {
    // Use smaller constants to avoid exceeding 2^16 limit
    // First compute dot product and keep it in reasonable range
    float h = dot(p, vec2(12.9898, 78.233));
    // Use mod to wrap large values, keeping within safe range
    h = mod(h, 1000.0);
    // Use smaller multiplier to ensure result stays well under 2^16
    // sin returns [-1, 1], so max result is 10000.0, well under 65536
    return fract(sin(h) * 10000.0);
}

// Smooth interpolation function
float smoothf(float t) {
    return t * t * (3.0 - 2.0 * t);
}

// 2D Perlin noise function
float perlin_noise(vec2 p) {
    // Get integer coordinates of the grid cell
    vec2 i = floor(p);
    vec2 f = fract(p);

    // Get hash values for the four corners
    float a = hash(i);
    float b = hash(i + vec2(1.0, 0.0));
    float c = hash(i + vec2(0.0, 1.0));
    float d = hash(i + vec2(1.0, 1.0));

    // Create gradient vectors from hash values
    vec2 grad_a = vec2(cos(a * 6.28318), sin(a * 6.28318));
    vec2 grad_b = vec2(cos(b * 6.28318), sin(b * 6.28318));
    vec2 grad_c = vec2(cos(c * 6.28318), sin(c * 6.28318));
    vec2 grad_d = vec2(cos(d * 6.28318), sin(d * 6.28318));

    // Distance vectors from corners to point
    vec2 dist_a = f;
    vec2 dist_b = f - vec2(1.0, 0.0);
    vec2 dist_c = f - vec2(0.0, 1.0);
    vec2 dist_d = f - vec2(1.0, 1.0);

    // Dot products (gradient * distance)
    float dot_a = dot(grad_a, dist_a);
    float dot_b = dot(grad_b, dist_b);
    float dot_c = dot(grad_c, dist_c);
    float dot_d = dot(grad_d, dist_d);

    // Smooth interpolation
    float u = smoothf(f.x);
    float v = smoothf(f.y);

    // Bilinear interpolation
    float x1 = mix(dot_a, dot_b, u);
    float x2 = mix(dot_c, dot_d, u);
    float result = mix(x1, x2, v);

    // Normalize to approximately [0, 1] range
    return result * 0.5 + 0.5;
}

// HSV to RGB conversion function
vec3 hsv_to_rgb(float h, float s, float v) {
    // h in [0, 1], s in [0, 1], v in [0, 1]
    float c = v * s;
    float x = c * (1.0 - abs(mod(h * 6.0, 2.0) - 1.0));
    float m = v - c;

    vec3 rgb;
    if (h < 1.0 / 6.0) {
        rgb = vec3(v, m + x, m);
    } else if (h < 2.0 / 6.0) {
        rgb = vec3(m + x, v, m);
    } else if (h < 3.0 / 6.0) {
        rgb = vec3(m, v, m + x);
    } else if (h < 4.0 / 6.0) {
        rgb = vec3(m, m + x, v);
    } else if (h < 5.0 / 6.0) {
        rgb = vec3(m + x, m, v);
    } else {
        rgb = vec3(v, m, m + x);
    }

    return rgb;
}

vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    // Center of texture
    vec2 center = outputSize * 0.5;

    // Direction from center to fragment
    vec2 dir = fragCoord - center;

    // Normalize coordinates to [0, 1] range for noise sampling
    vec2 uv = fragCoord / outputSize;

    // Zoom through noise using time with oscillation to stay bounded
    // Oscillate between minZoom and maxZoom to avoid unbounded growth
    float minZoom = 1.0;
    float maxZoom = 8.0;
    float zoomSpeed = 0.5;
    // Use sine to oscillate between min and max zoom
    // sin returns [-1, 1], map to [minZoom, maxZoom]
    float zoom = minZoom + (maxZoom - minZoom) * 0.5 * (sin(time * zoomSpeed) + 1.0);

    // Sample Perlin noise with zoom
    vec2 noiseCoord = uv * zoom;
    float noise = perlin_noise(noiseCoord);

    // Apply cosine to the noise and normalize to [0, 1] for hue
    float cosNoise = cos(noise * 6.28318); // Multiply by 2*PI for full cycle
    float hue = (cosNoise + 1.0) * 0.5; // Map from [-1, 1] to [0, 1]

    // Distance from center (normalized to [0, 1])
    float maxDist = length(outputSize * 0.5);
    float dist = length(dir) / maxDist;

    // Clamp distance to prevent issues
    dist = min(dist, 1.0);

    // Value (brightness): highest at center, darker at edges
    float value = 1.0 - dist * 0.5;

    // Convert HSV to RGB
    vec3 rgb = hsv_to_rgb(hue, 1.0, value);

    // Clamp to [0, 1] and return
    return vec4(max(vec3(0.0), min(vec3(1.0), rgb)), 1.0);
}
