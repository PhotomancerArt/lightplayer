// Veins — thin glowing lines tracing the contours of a slow noise field.
//
// family: Fields
// idea: ridged noise / marble veins and "noise isolines" (the zero-contour of a noise field drawn as a line), re-authored from scratch.
//
// Pattern space (`"coords": "pattern"`): `pos` is the lamps' box, centred,
// long side −1…1, y up.
//
// Rulers
//   size:   LED-relative — the vein network's cell is about nine lamps at
//           detail 1, and a vein is `thickness` lamps wide wherever it
//           runs (distance to the contour is |n| / |∇n|, so the width does
//           not swell where the field is flat). A vein pattern needs a few
//           lamps between lines to read at all, so it cannot be
//           piece-relative on a coarse matrix.
//   motion: LED-relative — the network crawls under a lamp a pitch every
//           couple of seconds and its gradients turn in place; exact loop
//           every `crawlPhase` period.

layout(binding = 0) uniform vec2 patternExtent;
layout(binding = 1) uniform float patternPitch;
layout(binding = 2) uniform float lampCount;
layout(binding = 3) uniform float detail;
layout(binding = 4) uniform float thickness;
layout(binding = 5) uniform float crawlPhase;
layout(binding = 6) uniform sampler2D palette;

const float TAU = 6.2831853;

vec3 pal(float u) {
    return texture(palette, vec2(clamp(u, 0.004, 0.996), 0.0)).rgb;
}

vec4 render_2d(vec2 pos) {
    float pitch = max(patternPitch, 0.004);
    float cell = 9.0 * pitch / clamp(detail, 0.1, 8.0);
    float f = 1.0 / cell;
    vec2 p = pos * f;

    float a = crawlPhase * TAU;
    vec2 crawl = vec2(cos(a), sin(a)) * 1.6;

    vec2 g;
    float n = lpfn_psrdnoise(p + crawl, vec2(0.0), a, g, 0u);
    // Distance to the zero contour, in pattern units.
    float gl = max(length(g), 0.3);
    float dist = abs(n) / (gl * f);

    float w = max(thickness, 0.25) * pitch;
    float core = 1.0 - smoothstep(0.25 * w, w, dist);
    float halo = exp(-min(dist / (1.8 * w), 12.0)) * 0.22;

    // A second, broader field shifts the colour along each vein.
    vec2 g2;
    float hueField = lpfn_psrdnoise(p * 0.35 - crawl * 0.5, vec2(0.0), -a, g2, 0u);
    float hue = 0.55 + 0.3 * hueField;

    vec3 ground = pal(0.02) * 0.10;
    vec3 c = ground + pal(hue) * core + pal(hue - 0.25) * halo;
    return vec4(clamp(c, 0.0, 1.0), 1.0);
}
