// Peach leaves — a green shimmer walking the twelve leaf lamps.
//
// The leaves are their own fixture, so they get their own shader: a slow
// swell with a faster sparkle on top of it. Both ride whole multiples of the
// same `phase` the body does, which is what keeps the two halves of the
// artwork breathing together even though nothing connects them but the bus.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;

vec4 render_1d(float pos) {
    float t = pos / outputSize.x;

    // The swell runs the length of the leaves and back.
    float swell = 0.5 + 0.5 * sin((t * 2.0 + phase * 3.0) * 6.2831853);
    // The sparkle is short and fast; squaring it keeps the crests tight.
    float sparkle = 0.5 + 0.5 * sin((t * 11.0 - phase * 7.0) * 6.2831853);
    float level = 0.30 + 0.50 * swell + 0.20 * sparkle * sparkle;

    return vec4(vec3(0.10, 0.82, 0.28) * level, 1.0);
}
