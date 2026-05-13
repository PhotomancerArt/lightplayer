void tick() {
    float phase_a = 0.5 + 0.5 * sin(time * 0.35);
    float phase_b = 0.5 + 0.5 * sin(time * 0.27 + 2.1);
    float breathe = 0.5 + 0.5 * sin(time * 0.18);

    emitters[0].id = 1u;
    emitters[0].pos = vec2(0.28 + phase_a * 0.44, 0.42);
    emitters[0].dir = vec2(1.0, 0.2);
    emitters[0].radius = 0.045;
    emitters[0].color = vec3(1.0, 0.25, 0.05);
    emitters[0].velocity = 0.10;
    emitters[0].intensity = 0.45;

    emitters[1].id = 2u;
    emitters[1].pos = vec2(0.72 - phase_b * 0.44, 0.58);
    emitters[1].dir = vec2(-0.6, -0.4);
    emitters[1].radius = 0.04;
    emitters[1].color = vec3(0.05, 0.45, 1.0);
    emitters[1].velocity = 0.08;
    emitters[1].intensity = 0.38;

    emitters[2].id = 3u;
    emitters[2].pos = vec2(0.5, 0.5);
    emitters[2].dir = vec2(0.0, 1.0);
    emitters[2].radius = 0.035 + breathe * 0.015;
    emitters[2].color = vec3(0.1, 1.0, 0.25);
    emitters[2].velocity = 0.04;
    emitters[2].intensity = 0.28;

    emitters[3].id = 0u;
}
