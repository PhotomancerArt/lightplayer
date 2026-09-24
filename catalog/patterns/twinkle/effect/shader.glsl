// Twinkle — individual lamps bloom and fade like sparks on a background colour.
//
// idea: fairy lights / FastLED "TwinkleFOX" and WLED "Twinklefox", re-
//       authored from scratch in closed form: every lamp has its own
//       hashed rhythm and a per-cycle coin toss, so nothing is stored.
//
// Pattern space (`"coords": "pattern"`): `pos` is the lamps' box, centred,
// long side −1…1, y up.
//
// Rulers
//   size:   LED-relative — sparks live on cells half a pitch wide, so at
//           detail 1 every lamp twinkles on its own; lower detail makes the
//           cells span several lamps and sparks bloom in little clusters.
//   motion: LED-relative in time — each cell sparks on its own rhythm of
//           3–6 chances per `twinklePhase` period (whole numbers, so the
//           loop is exact), blooming in ~0.4 s and fading over ~1.5 s.

layout(binding = 0) uniform vec2 patternExtent;
layout(binding = 1) uniform float patternPitch;
layout(binding = 2) uniform float lampCount;
layout(binding = 3) uniform float detail;
layout(binding = 4) uniform float density;
layout(binding = 5) uniform float twinklePhase;
layout(binding = 6) uniform sampler2D palette;

vec3 pal(float u) {
    return texture(palette, vec2(clamp(u, 0.004, 0.996), 0.0)).rgb;
}

vec4 render_2d(vec2 pos) {
    float pitch = max(patternPitch, 0.004);
    float cellSize = 0.5 * pitch / clamp(detail, 0.1, 8.0);
    // Keep the hash input small: Q16.16 has ±32768 of headroom and the
    // hash multiplies its input by ~80.
    vec2 key = floor(pos / cellSize) * 0.37;

    float r1 = lpfn_random(key, 0u);
    float r2 = lpfn_random(key + vec2(3.3, 7.1), 0u);
    float chances = 3.0 + floor(r2 * 3.999);

    float t = twinklePhase * chances + r1;
    float n = mod(floor(t), chances);
    float a = fract(t);

    float coin = lpfn_random(key + vec2(n * 1.31 + 0.5, n * 0.77 + 1.9), 0u);
    float on = step(coin, clamp(density, 0.0, 1.0));

    float bloom = smoothstep(0.0, 0.08, a);
    float fade = exp(-min(max(a - 0.08, 0.0) * 9.0, 12.0));
    float env = on * bloom * fade;

    vec3 ground = pal(0.02) * 0.45;
    vec3 spark = pal(0.62 + 0.3 * r1);
    return vec4(clamp(mix(ground, spark, env), 0.0, 1.0), 1.0);
}
