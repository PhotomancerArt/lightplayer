# Quad equal 100 v3 — the §12 capacity stress shape

Four EQUAL 100-LED channels through the real app path on the classic
ESP32 (DOM-Z-102). The engine free-runs, so with strips this long the
RMT transmits near back-to-back — the ISR-throughput worst case from
the ws281x findings §12 (the shape that hard-capped 32-word halves at 2
channels). M4-P3 measures trips/lag at 64-word halves against it.
