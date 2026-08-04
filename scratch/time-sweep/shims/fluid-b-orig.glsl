// Oracle shim: the fluid compute body's terms 5-7 of 7.
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;

vec4 render(vec2 pos) {
    float phase_c = time * 0.19 + 4.2;
    return vec4(
        sin(phase_c),
        sin(phase_c * 0.67),
        0.5 + 0.5 * sin(time * 0.18),
        0.0
    );
}
