layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;

vec3 rainbow(float t) {
    float r = 0.33333;
    vec3 v = abs(mod(fract(1.0 - t) + vec3(0.0, 1.0, 2.0) * r, 1.0) * 2.0 - 1.0);
    return v * v * (3.0 - 2.0 * v);
}

vec4 render(vec2 pos) {
    float led = 0.0;
    if (pos.x >= 1.0) {
        led = 1.0;
    }
    float hue = fract(time * 0.12 + led * 0.5);
    return vec4(rainbow(hue), 1.0);
}
