// Oracle shim: converted form of the fluid compute body's terms 5-7.
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float wave_c;
layout(binding = 2) uniform float wave_c2;
layout(binding = 3) uniform float wave_breathe;

const float TAU = 6.2831853;

vec4 render(vec2 pos) {
    return vec4(
        sin(TAU * wave_c),
        sin(TAU * wave_c2),
        0.5 + 0.5 * sin(TAU * wave_breathe),
        0.0
    );
}
