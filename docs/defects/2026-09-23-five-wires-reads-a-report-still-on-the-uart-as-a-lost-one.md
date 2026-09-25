---
status: fixed
found: 2026-09-23      # ci-equivalent local gate (`just test-emu-esp32v3-boot`), merging lean-wire (#791) into the BLE M3 access core (#794)
fixed: a3d928d4d
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

**Fix** — `a3d928d4d`, in the test's reading, with no bound or constant
moved. Two changes:

- The summary parser reads **complete lines only**: the text after the
  console's last newline is the line still in flight and is never parsed
  (`deinterleave` now keeps the console's final newline, and leaves a cut
  record whose tail never arrived unterminated). A half-sent line cut in its
  `crc=` could otherwise report a checksum the guest never printed.
- The guest's counter is read off the **report groups** (`reached`), not
  off each wire's own highest line. Groups must be 60 frames apart with
  none missing; every group but the last must carry exactly one line per
  wire; the last may be short only as a **prefix of the burst's order** —
  the burst the run stopped inside. Every wire then reached that frame, and
  the unchanged bounds `claimed <= decoded < claimed + 60` apply to it.

The bound's strength for real losses is kept and, in two ways, raised: a
line withheld from any finished group, a line missing from the *middle*
of the last burst, a wire reporting twice, and a whole missing group all
fail by name (`a_withheld_report_is_still_a_lost_one`, which fails when
the two group checks are disabled), and the lower bound now holds every
wire to the latest frame any wire reported. On the #794 image the walk
reads all five wires at 1980 against 1,982 pad frames and passes, all
three claims.

**Regression coverage** — `a_report_burst_the_run_stopped_inside_is_in_flight_not_lost`
replays this defect's own stream shape (cut in `first=` and cut in
`crc=`), and `a_withheld_report_is_still_a_lost_one` holds the teeth;
both run in plain `cargo test -p lp-emu-esp32v3`, no firmware needed.

**Lesson** — a bound between two streams that leave the machine at
different rates (pad frames at the render rate, their reports at the
UART's baud) needs the slower stream's in-flight time in it, or a deadline
that lands in the gap reads as a loss.

## Recurrence — 2026-09-24, the burst's FIRST line (PR #810)

The fix above covered a deadline inside a burst after at least one of its
lines was complete. On PR #810's `frame-dump` image (the BLE M4 branch) the
deadline lands one line earlier: the console's last text is
`[OUT] frame=1980 leds=16` with nothing after it, so no line of frame
1980's burst is complete, `reached` reads the 1920 group, and the pad has
carried 1,981 frames:

```
gpio18: the pad carried 1981 frames while the guest's last report was frame 1920 — more than the one report period (60) a deadline can fall inside, so summary lines are being lost
```

Same cause, same class — nothing is lost; the same test passes on
`origin/main` (`1cd1f7d4e`), whose image renders a slightly different
number of frames in the window. Fixed in the test's reading again, with no
bound or constant moved: `counted` wraps `reached` and also reads the
in-flight tail's `frame=` number — never its checksum, and only once the
character after the number has arrived — accepting it only as the burst
straight after a **whole** last group (after a short group it means that
group was finished with a line missing, and fails by name).
`a_burst_cut_in_its_first_line_is_in_flight_not_lost` replays this stream
(and the number cut mid-digit, and a non-adjacent frame);
`a_burst_in_flight_after_a_short_group_is_a_lost_report` holds the teeth.

The lesson above applied only halfway: the in-flight allowance has to reach
back to the burst's very first byte, not to its first newline.
