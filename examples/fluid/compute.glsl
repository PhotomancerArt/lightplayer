float phase;

void tick() {
    phase = phase + 0.035;
    if (phase > 1.0) {
        phase = phase - 1.0;
    }

    emitters[0].id = 1u;
    emitters[0].pos = vec2(0.35 + phase * 0.3, 0.42);
    emitters[0].dir = vec2(1.0, 0.2);
    emitters[0].radius = 0.08;
    emitters[0].color = vec3(1.0, 0.25, 0.05);
    emitters[0].velocity = 0.22;
    emitters[0].intensity = 1.4;

    emitters[1].id = 2u;
    emitters[1].pos = vec2(0.65 - phase * 0.25, 0.58);
    emitters[1].dir = vec2(-0.6, -0.4);
    emitters[1].radius = 0.07;
    emitters[1].color = vec3(0.05, 0.45, 1.0);
    emitters[1].velocity = 0.18;
    emitters[1].intensity = 1.2;

    emitters[2].id = 3u;
    emitters[2].pos = vec2(0.5, 0.5);
    emitters[2].dir = vec2(0.0, 1.0);
    emitters[2].radius = 0.05;
    emitters[2].color = vec3(0.1, 1.0, 0.25);
    emitters[2].velocity = 0.08;
    emitters[2].intensity = 0.8;

    emitters[3].id = 0u;
}
