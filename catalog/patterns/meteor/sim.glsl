// Meteor heads: per-meteor position/energy persisted across ticks in the
// produced map (compute globals are the engine's persistent state).
float prev_time;

void tick() {
    float active = clamp(count, 1.0, 4.0);

    if (meteors[0].id == 0u) {
        // Seed the clock alongside the table so the first tick advances by
        // zero rather than by the whole elapsed project time.
        prev_time = time;

        meteors[0].id = 1u;
        meteors[0].pos = vec2(0.0, 0.5);
        meteors[0].velocity = 0.22;
        meteors[0].color = vec3(1.0, 0.55, 0.15);
        meteors[0].radius = 0.14;

        meteors[1].id = 2u;
        meteors[1].pos = vec2(0.4, 0.5);
        meteors[1].velocity = 0.31;
        meteors[1].color = vec3(0.25, 0.55, 1.0);
        meteors[1].radius = 0.11;

        meteors[2].id = 3u;
        meteors[2].pos = vec2(0.7, 0.5);
        meteors[2].velocity = 0.26;
        meteors[2].color = vec3(0.4, 1.0, 0.35);
        meteors[2].radius = 0.12;

        meteors[3].id = 4u;
        meteors[3].pos = vec2(0.15, 0.5);
        meteors[3].velocity = 0.37;
        meteors[3].color = vec3(1.0, 0.3, 0.8);
        meteors[3].radius = 0.09;
    }

    // True elapsed time, deliberately UNCLAMPED: integrating constant
    // velocity over the real delta is exact at any frame rate, so the
    // meteors travel the same distance per second whether the tier renders
    // at 60fps or 1fps. A clamp here (there was one) throttles motion on
    // slow frames only, which reads as stutter on the CPU tier while the
    // GPU tier looks fine.
    float dt = max(time - prev_time, 0.0);
    prev_time = time;

    meteors[0].pos = vec2(mod(meteors[0].pos.x + meteors[0].velocity * speed * dt, 1.0), 0.5);
    meteors[1].pos = vec2(mod(meteors[1].pos.x + meteors[1].velocity * speed * dt, 1.0), 0.5);
    meteors[2].pos = vec2(mod(meteors[2].pos.x + meteors[2].velocity * speed * dt, 1.0), 0.5);
    meteors[3].pos = vec2(mod(meteors[3].pos.x + meteors[3].velocity * speed * dt, 1.0), 0.5);

    meteors[0].intensity = 1.0;
    meteors[1].intensity = 0.0;
    meteors[2].intensity = 0.0;
    meteors[3].intensity = 0.0;
    if (active >= 2.0) {
        meteors[1].intensity = 1.0;
    }
    if (active >= 3.0) {
        meteors[2].intensity = 1.0;
    }
    if (active >= 4.0) {
        meteors[3].intensity = 1.0;
    }
}
