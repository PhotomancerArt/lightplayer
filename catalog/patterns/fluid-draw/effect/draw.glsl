// Touches -> fluid emitters, in pattern space (the lamps' box, centred,
// long side -1..1, y up). The Fluid node reads them in the same space
// ("coords": "pattern"), so nothing converts in between.
//
// Each finger stamps STEPS emitters along the chord from where it was last
// tick to where it is now: Studio sends at most ~30 samples a second, so a
// fast stroke arrives as chords, and stamping along them keeps the line
// whole (lp2014's Touch.interpolateInt). Velocity and direction come from
// the same chord. A new finger has no chord: it dabs once.
//
// Colour comes from where a stroke starts (lp2014's hue formula) and stays
// with that finger until it lifts.
//
// When nobody has touched for `IDLE_AFTER` seconds, three wandering
// "friends" (projects/test/fluid's choreography) fade in, and fade out
// again within a third of a second of the next touch.
//
// Globals persist across ticks (they are the shader's state).

const float TAU = 6.2831853;
const int FINGERS = 4;
const int STEPS = 4;
const int FRIENDS = 3;
// emitters[0..2] are the friends (ids 1..3); emitters[3..18] the strokes
// (ids 16 + finger * STEPS + step). Ids never collide, so the Fluid node's
// by-key merge keeps them apart.
const int STROKE_BASE = 3;
const uint STROKE_ID_BASE = 16u;
// The pattern's own settings (not knobs): the stroke's radius in pattern
// units (the lamps' long side is 2), and how long the fluid must go
// untouched before the friends come back. Kept as constants: every declared
// input costs the node's definition about a kilobyte on the wire, and a
// board sends a definition in one frame.
const float BRUSH = 0.12;
const float IDLE_AFTER = 2.0;

uint prev_id[4];
vec2 prev_pos[4];
float prev_hue[4];
float prev_time;
float last_touch;
float friends;
bool seeded;

vec3 hsv(float h, float s, float v) {
    vec3 k = clamp(abs(fract(h + vec3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0, 0.0, 1.0);
    return v * mix(vec3(1.0), k, s);
}

float start_hue(vec2 p) {
    // lp2014: (initialY * 32 + initialX) mod 1, on 0..1 picture coords.
    vec2 q = p * 0.5 + 0.5;
    return fract(q.y * 32.0 + q.x);
}

void clear_emitter(int i) {
    emitters[i].id = 0u;
}

void tick() {
    // The pattern's two public knobs live here, where an unwritten channel
    // falls back to its default; the Fluid node reads them from us.
    fluid_fade = fade;
    fluid_viscosity = viscosity;

    if (!seeded) {
        seeded = true;
        prev_time = time;
        last_touch = time - IDLE_AFTER;
        friends = 1.0;
    }
    float dt = clamp(time - prev_time, 0.0, 0.25);
    prev_time = time;

    // Uniform struct arrays take constant indices only.
    uint tid[4];
    vec2 tpos[4];
    tid[0] = touches[0].id; tpos[0] = touches[0].pos;
    tid[1] = touches[1].id; tpos[1] = touches[1].pos;
    tid[2] = touches[2].id; tpos[2] = touches[2].pos;
    tid[3] = touches[3].id; tpos[3] = touches[3].pos;

    bool any_touch = tid[0] != 0u || tid[1] != 0u || tid[2] != 0u || tid[3] != 0u;
    if (any_touch) {
        last_touch = time;
    }

    uint next_id[4];
    vec2 next_pos[4];
    float next_hue[4];
    for (int k = 0; k < FINGERS; k++) {
        next_id[k] = tid[k];
        next_pos[k] = tpos[k];
        next_hue[k] = 0.0;
        int base = STROKE_BASE + k * STEPS;
        if (tid[k] == 0u) {
            for (int s = 0; s < STEPS; s++) {
                clear_emitter(base + s);
            }
            continue;
        }
        // Continuing stroke: this finger was here last tick.
        vec2 from = tpos[k];
        float hue = start_hue(tpos[k]);
        bool continuing = false;
        for (int j = 0; j < FINGERS; j++) {
            if (prev_id[j] == tid[k]) {
                from = prev_pos[j];
                hue = prev_hue[j];
                continuing = true;
            }
        }
        next_hue[k] = hue;
        vec2 chord = tpos[k] - from;
        float len = length(chord);
        vec2 dir = len > 0.0001 ? chord / len : vec2(0.0, 1.0);
        float speed = dt > 0.0 ? len / dt : 0.0;
        vec3 color = hsv(hue, 0.85, 1.0);
        for (int s = 0; s < STEPS; s++) {
            int i = base + s;
            // A new finger dabs once (only the last step); a moving one
            // stamps every step along the chord.
            if (!continuing && s < STEPS - 1) {
                clear_emitter(i);
                continue;
            }
            float t = float(s + 1) / float(STEPS);
            emitters[i].id = STROKE_ID_BASE + uint(k * STEPS + s);
            emitters[i].pos = mix(from, tpos[k], t);
            emitters[i].dir = dir;
            emitters[i].radius = BRUSH;
            emitters[i].color = color;
            emitters[i].velocity = min(speed * 0.01, 0.05);
            emitters[i].intensity = 0.5;
        }
    }
    for (int k = 0; k < FINGERS; k++) {
        prev_id[k] = next_id[k];
        prev_pos[k] = next_pos[k];
        prev_hue[k] = next_hue[k];
    }

    // Friends: in after `IDLE_AFTER` s without a touch, out fast on one.
    float target = (time - last_touch) >= IDLE_AFTER ? 1.0 : 0.0;
    float rate = target > friends ? 1.0 : 3.0;
    friends = clamp(friends + (target - friends) * min(dt * rate * 4.0, 1.0), 0.0, 1.0);
    if (friends < 0.01) {
        clear_emitter(0);
        clear_emitter(1);
        clear_emitter(2);
    } else {
        // One phasor per friend; each wanders a Lissajous path off it, its
        // second axis an integer multiple so the path stays periodic.
        vec2 centre = vec2(0.0, 0.0);
        float a = TAU * wave_a;
        float b = TAU * wave_b;
        float c = TAU * wave_c;

        emitters[0].id = 1u;
        emitters[0].pos = vec2(-0.76 + 0.24 * sin(a), 0.30 * sin(2.0 * a + 0.7));
        emitters[0].dir = normalize(centre - emitters[0].pos + vec2(0.0001, 0.0));
        emitters[0].radius = 0.25;
        emitters[0].color = vec3(1.0, 0.25, 0.05);
        emitters[0].velocity = 0.02;
        emitters[0].intensity = 0.40 * friends;

        emitters[1].id = 2u;
        emitters[1].pos = vec2(0.76 + 0.16 * sin(2.0 * b + 1.3), 0.30 * sin(b));
        emitters[1].dir = normalize(centre - emitters[1].pos + vec2(0.0001, 0.0));
        emitters[1].radius = 0.2;
        emitters[1].color = vec3(0.05, 0.45, 1.0);
        emitters[1].velocity = 0.014;
        emitters[1].intensity = 0.36 * friends;

        emitters[2].id = 3u;
        emitters[2].pos = vec2(0.68 * sin(c), -0.30 + 0.12 * sin(3.0 * c + 0.4));
        emitters[2].dir = normalize(centre - emitters[2].pos + vec2(0.0001, 0.0));
        emitters[2].radius = 0.17 + 0.04 * (0.5 + 0.5 * sin(2.0 * c));
        emitters[2].color = vec3(0.1, 1.0, 0.25);
        emitters[2].velocity = 0.006;
        emitters[2].intensity = 0.32 * friends;
    }
    clear_emitter(19);
}
