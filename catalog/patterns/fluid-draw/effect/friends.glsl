// The idle "friends": three slow emitters that stir the fluid when nobody
// is drawing (projects/test/fluid's choreography, in pattern space). They
// fade in after IDLE_AFTER seconds without a touch and fade out within a
// third of a second of the next one. Ids 1..3: the strokes are 16 and up.
//
// One phasor per friend; each wanders a Lissajous path off it, its second
// axis an integer multiple so the path stays periodic.

const float TAU = 6.2831853;
// How long the fluid must go untouched before the friends come back. A
// constant, not a knob (see draw.glsl on what an input costs).
const float IDLE_AFTER = 2.0;

float prev_time;
float last_touch;
float level;
bool seeded;

void friend(int i, uint id, vec2 pos, float radius, vec3 color, float velocity, float intensity) {
    emitters[i].id = level < 0.01 ? 0u : id;
    emitters[i].pos = pos;
    emitters[i].dir = normalize(-pos + vec2(0.0001, 0.0));
    emitters[i].radius = radius;
    emitters[i].color = color;
    emitters[i].velocity = velocity;
    emitters[i].intensity = intensity * level;
}

void tick() {
    if (!seeded) {
        seeded = true;
        prev_time = time;
        last_touch = time - IDLE_AFTER;
        level = 1.0;
    }
    float dt = clamp(time - prev_time, 0.0, 0.25);
    prev_time = time;
    if (touches[0].id != 0u || touches[1].id != 0u || touches[2].id != 0u
        || touches[3].id != 0u) {
        last_touch = time;
    }
    float target = (time - last_touch) >= IDLE_AFTER ? 1.0 : 0.0;
    float rate = target > level ? 4.0 : 12.0;
    level = clamp(level + (target - level) * min(dt * rate, 1.0), 0.0, 1.0);

    float a = TAU * wave_a;
    float b = TAU * wave_b;
    float c = TAU * wave_c;
    friend(0, 1u, vec2(-0.76 + 0.24 * sin(a), 0.30 * sin(2.0 * a + 0.7)), 0.25,
        vec3(1.0, 0.25, 0.05), 0.02, 0.40);
    friend(1, 2u, vec2(0.76 + 0.16 * sin(2.0 * b + 1.3), 0.30 * sin(b)), 0.2,
        vec3(0.05, 0.45, 1.0), 0.014, 0.36);
    friend(2, 3u, vec2(0.68 * sin(c), -0.30 + 0.12 * sin(3.0 * c + 0.4)),
        0.17 + 0.04 * (0.5 + 0.5 * sin(2.0 * c)), vec3(0.1, 1.0, 0.25), 0.006, 0.32);
}
