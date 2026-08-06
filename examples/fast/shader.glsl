layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;

vec4 render(vec2 pos) {
    return vec4(phase, 0.0, 0.0, 1.0);
}
