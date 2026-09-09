// Emitter choreography: every term used to be `sin(time * k [+ c])`, which
// needed unbounded seconds inside the shader. Each is now its own phasor —
// a wrapped cycle position with the constant folded into the phase offset —
// so the pattern is identical and nothing overflows.
const float TAU = 6.2831853;

void tick() {
    vec2 center = vec2(0.5, 0.5);
    float breathe = 0.5 + 0.5 * sin(TAU * wave_breathe);

    emitters[0].id = 1u;
    emitters[0].pos = vec2(0.12 + 0.12 * sin(TAU * wave_a), 0.50 + 0.34 * sin(TAU * wave_a2));
    emitters[0].dir = center - emitters[0].pos;
    emitters[0].radius = 0.125;
    emitters[0].color = vec3(1.0, 0.25, 0.05);
    emitters[0].velocity = 0.01;
    emitters[0].intensity = 0.40;

    emitters[1].id = 2u;
    emitters[1].pos = vec2(0.88 + 0.08 * sin(TAU * wave_b2), 0.50 + 0.36 * sin(TAU * wave_b));
    emitters[1].dir = center - emitters[1].pos;
    emitters[1].radius = 0.1;
    emitters[1].color = vec3(0.05, 0.45, 1.0);
    emitters[1].velocity = 0.007;
    emitters[1].intensity = 0.36;

    emitters[2].id = 3u;
    emitters[2].pos = vec2(0.50 + 0.34 * sin(TAU * wave_c), 0.12 + 0.12 * sin(TAU * wave_c2));
    emitters[2].dir = center - emitters[2].pos;
    emitters[2].radius = 0.085 + breathe * 0.02;
    emitters[2].color = vec3(0.1, 1.0, 0.25);
    emitters[2].velocity = 0.003;
    emitters[2].intensity = 0.32;

    emitters[3].id = 0u;
}
