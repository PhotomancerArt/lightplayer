// Scanner — a soft bar sweeping back and forth with a fading tail.
//
// family: Fronts
// idea: the Larson scanner / "KITT" sweep (WLED "Scanner"), expanded to a bar across 2D and re-authored in closed form: the tail is the time since the bar last passed a lamp, so it is exact at any frame rate and needs no per-lamp history.
//
// Pattern space (`"coords": "pattern"`): `pos` is the lamps' box, centred,
// long side −1…1, y up.
//
// Rulers
//   size:   LED-relative — the bar is `width` lamps wide on any piece.
//   motion: piece-relative — the bar crosses the lamp box's long side
//           (x) edge to edge once per half `sweepPhase` period, there and
//           back. The tail's length is a fraction of that sweep, so it
//           keeps its look on a longer piece.

layout(binding = 0) uniform vec2 patternExtent;
layout(binding = 1) uniform float patternPitch;
layout(binding = 2) uniform float lampCount;
layout(binding = 3) uniform float detail;
layout(binding = 4) uniform float width;
layout(binding = 5) uniform float tail;
layout(binding = 6) uniform float sweepPhase;
layout(binding = 7) uniform sampler2D palette;

vec3 pal(float u) {
    return texture(palette, vec2(clamp(u, 0.004, 0.996), 0.0)).rgb;
}

vec4 render_2d(vec2 pos) {
    float pitch = max(patternPitch, 0.004);
    float ex = max(patternExtent.x, pitch);
    // Where this lamp sits along the sweep, 0 at the left edge, 1 at the right.
    float u = clamp(0.5 + 0.5 * pos.x / ex, 0.0, 1.0);

    // Triangle sweep: out on the first half of the cycle, back on the second.
    float ph = sweepPhase;
    float b = ph < 0.5 ? 2.0 * ph : 2.0 - 2.0 * ph;
    float barX = (2.0 * b - 1.0) * ex;

    float w = max(width, 0.5) * pitch / clamp(detail, 0.1, 8.0);
    float d = (pos.x - barX) / w;
    float core = exp(-min(d * d, 16.0));

    // The bar passes this lamp at ph = u/2 (outbound) and 1 - u/2 (return).
    float since = min(fract(ph - 0.5 * u + 1.0), fract(ph - (1.0 - 0.5 * u) + 1.0));
    float len = clamp(tail, 0.02, 1.0) * 0.5;
    float trail = exp(-min(since / len * 3.0, 12.0));

    vec3 c = pal(0.45 + 0.4 * core) * core + pal(0.08 + 0.55 * trail) * trail * 0.75;
    return vec4(clamp(c, 0.0, 1.0), 1.0);
}
