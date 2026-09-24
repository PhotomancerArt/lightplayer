// Ripples — rings spawn at random spots and spread outward, fading as they go.
//
// idea: raindrops on a pond (WLED "Ripple" / "Ripple Rainbow"), expanded
//       to 2D and re-authored in closed form: each ring slot has a fixed
//       phase offset, and its birthplace is a hash of the slot and the
//       ring's cycle number, so no state is kept between frames.
//
// Pattern space (`"coords": "pattern"`): `pos` is the lamps' box, centred,
// long side −1…1, y up.
//
// Rulers
//   size:   the ring's thickness is LED-relative (about 1.3 lamps, thicker
//           as it ages); how far a ring spreads is piece-relative (to 0.9
//           of the long half-side), and birthplaces cover the lamp box.
//   motion: piece-relative — each ring lives 3 s; eight lifetimes per
//           `rainPhase` period, exact loop.

layout(binding = 0) uniform vec2 patternExtent;
layout(binding = 1) uniform float patternPitch;
layout(binding = 2) uniform float lampCount;
layout(binding = 3) uniform float detail;
layout(binding = 4) uniform float rate;
layout(binding = 5) uniform float rainPhase;
layout(binding = 6) uniform sampler2D palette;

const float LIVES = 8.0;

vec3 pal(float u) {
    return texture(palette, vec2(clamp(u, 0.004, 0.996), 0.0)).rgb;
}

vec4 render_2d(vec2 pos) {
    float pitch = max(patternPitch, 0.004);
    float n = clamp(floor(rate + 0.5), 1.0, 6.0);
    float reach = 0.9 / clamp(detail, 0.1, 8.0);
    vec3 c = pal(0.0) * 0.05;

    for (int i = 0; i < 6; i++) {
        float fi = float(i);
        if (fi >= n) {
            break;
        }
        float t = rainPhase * LIVES + fi / n;
        // Mod LIVES: a slot's ring that straddles the phasor's wrap keeps
        // its birthplace (cycle 8 and cycle 0 are the same ring).
        float cycle = mod(floor(t), LIVES);
        float age = fract(t);

        // Birthplace: two hashes of (slot, cycle), spread over the box.
        vec2 key = vec2(fi * 1.37 + 0.11, cycle * 0.73 + 0.29);
        float hx = lpfn_random(key, 0u);
        float hy = lpfn_random(key + vec2(4.1, 2.3), 0u);
        vec2 centre = (vec2(hx, hy) * 2.0 - 1.0) * patternExtent * 0.85;

        float radius = age * reach;
        float thick = pitch * (1.3 + 1.5 * age);
        float d = (length(pos - centre) - radius) / thick;
        float ring = exp(-min(d * d, 16.0));
        float fade = (1.0 - age) * (1.0 - age);
        // A soft flash at the birthplace for the first moment.
        float birth = exp(-min(length(pos - centre) / (2.0 * pitch), 8.0)) * max(0.0, 1.0 - age * 6.0);

        float hue = 0.35 + 0.55 * lpfn_random(key + vec2(9.7, 5.9), 0u);
        c += pal(hue) * (ring * fade + birth * 0.6);
    }
    return vec4(clamp(c, 0.0, 1.0), 1.0);
}
