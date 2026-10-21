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
// with that finger until it lifts. The idle "friends" are `friends.glsl`,
// a shader of their own: each compiles within the board's compile budget.
//
// Globals persist across ticks (they are the shader's state).

const int FINGERS = 4;
const int STEPS = 4;
// emitters[k * STEPS + s]: finger slot k, step s, id 16 + k * STEPS + s.
// The friends are ids 1..3, so the Fluid node's by-key merge keeps them apart.
const uint STROKE_ID_BASE = 16u;
// The stroke's radius in pattern units (the lamps' long side is 2). A
// constant, not a knob: every declared input costs the node's definition
// about a kilobyte on the wire, and a board sends a definition in one frame.
const float BRUSH = 0.12;

// Each finger's id, position and hue as of the last tick, and this tick's,
// by touch slot.
uint prev_id[4];
vec2 prev_pos[4];
float prev_hue[4];
uint next_id[4];
vec2 next_pos[4];
float next_hue[4];
float prev_time;
bool seeded;

vec3 hsv(float h, float s, float v) {
    vec3 k = clamp(abs(fract(h + vec3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0, 0.0, 1.0);
    return v * mix(vec3(1.0), k, s);
}

// One finger, slot `k`, at `pos` this tick: stamp its chord since last tick.
void stamp_finger(int k, uint id, vec2 pos, float dt) {
    int base = k * STEPS;
    next_id[k] = id;
    next_pos[k] = pos;
    next_hue[k] = 0.0;
    if (id == 0u) {
        for (int s = 0; s < STEPS; s++) {
            emitters[base + s].id = 0u;
        }
        return;
    }
    // A continuing stroke: this finger was here last tick. A new one takes
    // its hue from where it lands: lp2014's (y * 32 + x) mod 1 on 0..1.
    vec2 from = pos;
    vec2 q = pos * 0.5 + 0.5;
    float hue = fract(q.y * 32.0 + q.x);
    bool continuing = false;
    for (int j = 0; j < FINGERS; j++) {
        if (prev_id[j] == id) {
            from = prev_pos[j];
            hue = prev_hue[j];
            continuing = true;
        }
    }
    next_hue[k] = hue;
    vec2 chord = pos - from;
    float len = length(chord);
    vec2 dir = len > 0.0001 ? chord / len : vec2(0.0, 1.0);
    float speed = dt > 0.0 ? len / dt : 0.0;
    vec3 color = hsv(hue, 0.85, 1.0);
    for (int s = 0; s < STEPS; s++) {
        int i = base + s;
        // A new finger dabs once (the last step); a moving one stamps every
        // step along the chord.
        bool stamp = continuing || s == STEPS - 1;
        emitters[i].id = stamp ? STROKE_ID_BASE + uint(i) : 0u;
        emitters[i].pos = mix(from, pos, float(s + 1) / float(STEPS));
        emitters[i].dir = dir;
        emitters[i].radius = BRUSH;
        emitters[i].color = color;
        emitters[i].velocity = min(speed * 0.01, 0.05);
        emitters[i].intensity = 0.5;
    }
}

void tick() {
    // The pattern's two public knobs live here, where an unwritten channel
    // falls back to its default; the Fluid node reads them from us.
    fluid_fade = fade;
    fluid_viscosity = viscosity;

    if (!seeded) {
        seeded = true;
        prev_time = time;
    }
    float dt = clamp(time - prev_time, 0.0, 0.25);
    prev_time = time;

    // Uniform struct arrays take constant indices only.
    stamp_finger(0, touches[0].id, touches[0].pos, dt);
    stamp_finger(1, touches[1].id, touches[1].pos, dt);
    stamp_finger(2, touches[2].id, touches[2].pos, dt);
    stamp_finger(3, touches[3].id, touches[3].pos, dt);
    for (int k = 0; k < FINGERS; k++) {
        prev_id[k] = next_id[k];
        prev_pos[k] = next_pos[k];
        prev_hue[k] = next_hue[k];
    }
}
