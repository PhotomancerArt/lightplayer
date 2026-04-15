// test run

// Lygia physical hue (neon rainbow)
vec3 paletteRainbow() {
    float t = 1.0;
    float r = 0.33333;
    vec3 v = abs(mod(fract(1.0 - t) + vec3(0.0, 1.0, 2.0) * r, 1.0) * 2.0 - 1.0);
    return v * v * (3.0 - 2.0 * v);
}
// run: paletteRainbow() ~= vec3(1.0, 0.25926208, 0.2591858) (tolerance: 0.002)
