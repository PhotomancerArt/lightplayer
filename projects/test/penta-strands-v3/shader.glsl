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

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    float band = floor(uv.y * 5.0);

    // `phase` is one 20 s cycle; each band rides a whole-number multiple of
    // it (0.25 … 0.65 Hz originally), which keeps the band relation exact
    // without carrying unbounded seconds into the shader.
    float head = fract(phase * (5.0 + band * 2.0) + band * 0.2);
    float d = abs(uv.x - head);
    d = min(d, 1.0 - d);
    float dot_i = smoothstep(0.3, 0.0, d);

    vec3 color = bandColor(band) * (0.25 + 0.75 * dot_i);
    return vec4(color, 1.0);
}
