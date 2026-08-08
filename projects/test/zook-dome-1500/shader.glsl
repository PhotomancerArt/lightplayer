// Penta-strand bring-up pattern: five horizontal bands, one per strand.
//
// One fixture authors five paths and one output node splits its single
// control buffer across five wires, so every band must land on its own strip:
// a strand wired to the wrong pin, or a slice computed off by a lamp, shows
// up immediately as the wrong color. Each band also carries a chase dot at a
// band-specific speed and phase, so a slice that overlaps its neighbour is
// visible as two dots moving together.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;

vec3 bandColor(float band) {
    if (band < 0.5) {
        return vec3(1.0, 0.0, 0.0);
    }
    if (band < 1.5) {
        return vec3(0.0, 1.0, 0.0);
    }
    if (band < 2.5) {
        return vec3(0.0, 0.0, 1.0);
    }
    if (band < 3.5) {
        return vec3(1.0, 0.6, 0.0);
    }
    return vec3(0.7, 0.0, 1.0);
}

vec4 render_2d(vec2 pos) {
    vec2 uv = pos / outputSize;
    float band = floor(uv.y * 5.0);

    // Band speeds 0.25..0.65 Hz are the 5,7,9,11,13 harmonics of the
    // authored 20 s phasor; fract(k*phase) is continuous under the wrap
    // because k is an integer (the quad-strips doctrine).
    float harmonic = 5.0 + band * 2.0;
    float head = fract(phase * harmonic + band * 0.2);
    float d = abs(uv.x - head);
    d = min(d, 1.0 - d);
    float dot_i = smoothstep(0.3, 0.0, d);

    vec3 color = bandColor(band) * (0.25 + 0.75 * dot_i);
    return vec4(color, 1.0);
}
