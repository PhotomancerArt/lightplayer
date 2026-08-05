void tick() {
    uint phase = uint(time * 2.0);

    if ((phase % 5u) == 1u) {
        events[0].id = 2u;
        events[0].seq = phase + 21u;
    } else {
        events[0].id = 0u;
        events[0].seq = 0u;
    }

    if ((phase % 7u) == 4u) {
        events[1].id = 6u;
        events[1].seq = phase + 26u;
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
