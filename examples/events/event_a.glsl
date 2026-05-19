void tick() {
    uint phase = uint(time * 2.0);

    if ((phase % 4u) == 0u) {
        events[0].id = 1u;
        events[0].seq = phase + 11u;
    } else {
        events[0].id = 0u;
        events[0].seq = 0u;
    }

    if ((phase % 6u) == 3u) {
        events[1].id = 4u;
        events[1].seq = phase + 14u;
    } else {
        events[1].id = 0u;
        events[1].seq = 0u;
    }

    events[2].id = 0u;
    events[2].seq = 0u;
    events[3].id = 0u;
    events[3].seq = 0u;
    events[4].id = 0u;
    events[4].seq = 0u;
    events[5].id = 0u;
    events[5].seq = 0u;
    events[6].id = 0u;
    events[6].seq = 0u;
    events[7].id = 0u;
    events[7].seq = 0u;
}
