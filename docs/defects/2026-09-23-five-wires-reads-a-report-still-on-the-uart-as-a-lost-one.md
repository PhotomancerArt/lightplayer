---
status: open
found: 2026-09-23      # ci-equivalent local gate (`just test-emu-esp32v3-boot`), merging lean-wire (#791) into the BLE M3 access core (#794)
area: lp-emu/esp/lp-emu-esp32v3/tests/five_wires.rs (the "no summary line is lost" bound)
class: assumed-context
related:
  - lp2025/2026-09-23-1428-ble-remote-control/m3-access-core.md
  - lp-emu/esp/lp-emu-esp32v3/tests/five_wires.rs
---
# five_wires reads a report still leaving UART0 at the deadline as a lost one

**Symptom** — `the_five_wire_walk_holds_on_routing_checksums_and_determinism`
fails on the merged tree's `frame-dump` image (PR #794 over `ebf63d463`):

```
gpio14: the pad carried 1982 frames while the guest's last report was frame 1920 — more than the one report period (60) a deadline can fall inside, so summary lines are being lost
```

Every pad decodes 1,982 complete frames with no bit errors; routing,
checksums and the quantum-64 comparison pass. The same test passes on the
M3 branch before the merge (2,000 frames, every pad's last report 1980) and
on `origin/main` `ebf63d463` (1,986–1,987 frames, every last report 1980).

**Root cause** — nothing is lost. The guest prints one `[OUT] frame=N`
summary per wire every 60 frames, five lines of ~120 B each, over UART0 at
921,600 baud in emulated time. The 10 s deadline on this image lands two
frames after frame 1980, while those five lines are still going out: the
captured UART0 stream ends mid-line, in the second of them —

```
[INFO] fw_esp32v3::output::rmt::frame_dump: [OUT] frame=1980 leds=16 crc=0x19e6e98d lit=16 first=(42,79,5) (1,84,64) (26,31,117) (61,11,126)
[INFO] fw_esp32v3::output::rmt::frame_dump: [OUT] frame=1980 leds=16 crc=0xda7f5d46 lit=16 first=(100,1,87)
```

— so gpio14, gpio2 and gpio13 have no 1980 line yet and read back as 1920,
62 frames behind the pad. The bound `decoded < claimed + 60` assumes every
report the guest has made is on the wire when the run stops; it is not
when the deadline falls within the few milliseconds the report takes to
drain. The same file already says this about the output opens
(`OPEN_LINE`: "the later opens are still in the TX FIFO when the guest
stops"). Lean-wire slowed the render loop from ~2,000 to ~1,986 frames in
the window and the access core ~4 more; 1,982 is the first count that puts
the deadline inside a report's drain.

**Fix** — none yet: a gate's tolerance is not this merge's to widen. The
shape a fix takes is the test's to choose — count a report as made once
its frame is on the pad and its line has had time to drain, or stop the
run on a quiet UART rather than a bare deadline — not a larger constant.

**Regression coverage** — the failing test is the coverage; it fails
deterministically on this image.

**Lesson** — a bound between two streams that leave the machine at
different rates (pad frames at the render rate, their reports at the
UART's baud) needs the slower stream's in-flight time in it, or a deadline
that lands in the gap reads as a loss.
