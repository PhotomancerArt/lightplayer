// Peach body — a pink plane, blushing from the bottom up, with a sheen
// sweeping across it.
//
// The same artwork as `examples/peach-1d`, declared the other way. This is a
// picture painted over the whole peach, and the fixture's mapping decides
// which part of it each lamp gets. Nothing here knows the strand order, and
// the sheen crosses the two legs of the body at the same moment because they
// are next to each other in SPACE — the wire order has nothing to say about
// it.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;
layout(binding = 2) uniform float glow;

vec4 render_2d(vec2 pos) {
    vec2 uv = pos / outputSize;

    // Ripe and deep at the bottom of the fruit, lighter toward the stem.
    float ripeness = 1.0 - uv.y;
    vec3 flesh = mix(vec3(1.0, 0.66, 0.50), vec3(0.96, 0.30, 0.40), ripeness);

    // One diagonal sheen per cycle, `glow` wide.
    float sweep = sin(((uv.x * 0.6 + uv.y * 0.4) - phase) * 6.2831853);
    float sheen = exp(-(1.0 - sweep) / max(glow, 0.05));

    vec3 lit = flesh * (0.55 + 0.30 * ripeness) + vec3(1.0, 0.88, 0.84) * sheen * 0.45;
    return vec4(lit, 1.0);
}
