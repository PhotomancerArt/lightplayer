// test run

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;

vec4 render(vec2 pos) {
    return vec4(mod(time, 1.0), 0.0, 0.0, 1.0);
}

// set_uniform: time = 2.25
// run: render(vec2(0.0, 0.0)) ~= vec4(0.25, 0.0, 0.0, 1.0) (tolerance: 0.002)
