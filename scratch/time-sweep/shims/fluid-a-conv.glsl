// Oracle shim: converted form of the fluid compute body's terms 1-4.
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float wave_a;
layout(binding = 2) uniform float wave_a2;
layout(binding = 3) uniform float wave_b2;
layout(binding = 4) uniform float wave_b;

const float TAU = 6.2831853;

vec4 render(vec2 pos) {
    return vec4(
        sin(TAU * wave_a),
        sin(TAU * wave_a2),
        sin(TAU * wave_b2),
        sin(TAU * wave_b)
    );
}
