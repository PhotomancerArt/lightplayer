layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;

// The plainest shader: every lamp breathes one colour on a phasor.
// A raised cosine so the floor is never black — a dark strip on the
// bench must mean a fault, not the trough of the animation.
vec4 render_2d(vec2 pos) {
    float level = 0.15 + 0.85 * (0.5 - 0.5 * cos(phase * 6.2831853));
    return vec4(0.1 * level, 0.5 * level, 1.0 * level, 1.0);
}
