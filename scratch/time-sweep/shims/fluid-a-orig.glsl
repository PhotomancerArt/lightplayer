// Oracle shim: the fluid compute body's emitter-choreography terms, lifted
// into a render() so lps-probe can evaluate them. Terms 1-4 of 7.
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;

vec4 render(vec2 pos) {
    float phase_a = time * 0.31;
    float phase_b = time * 0.23 + 2.1;
    return vec4(
        sin(phase_a),
        sin(phase_a * 0.73),
        sin(phase_b * 0.81),
        sin(phase_b)
    );
}
