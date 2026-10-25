---
status: open
found: 2026-10-22      # test (lp-emu:esp32s3:t1, M7 P10's U1, emu_s3_ota.rs)
area: fw-esp32s3 / lp-link — the first link session after an update's reset
class: silent-drop
related:
  - lp2025/2026-10-20-2335-s3-radio-and-ota (M7 P10 found it)
---
# The S3 loses its `[CORE]` line after an update's reset

**Symptom** — On the emulated S3, X → Y over the USB link ends `UpToDate`
and the board runs Y (the loader says `[LOADER] core @0x560000 (proven)`, the
engine's hello names Y, `running`), but the core's own boot line —
`[CORE] core @0x560000 +… (proven) build … · engine … B · digest …` — never
reaches the host. The board's link, up as session 0 after the software reset
the update ends with, resets at once:

```
[link] reset (PeerRestarted)
[link] up (session 3)
[usb_link] host link reset (ProtocolError); now session 1
M!{"id":0,"msg":{"hello":{… "buildId":"b1b1b1b1+…","state":"running" …}}}
```

A fresh boot of the same image says the line (just after `[link] up`), and
so do the S3's warm reboots in the split scenarios (S3, S12), where the host
is quiet when the board resets. The C6's update scenarios read the same line
and pass.

**Why it is lost** — the core logs it through the log ring onto the link's
log channel, which is best-effort datagrams (`lp-link`'s `pump_log`), and a
link reset clears queued datagrams (`Link::reset`: `datagrams.clear()`). The
`ProtocolError` that resets the board's first session is not chased here:
the candidates are `lp-link`'s ack-beyond-anything-sent check
(`on_ack_fields`) and its fragment check (`Inbox::push_fragment`), fed by
host frames that were queued before the reboot and reach the new session.
Whether silicon does the same depends on what the S3's USB-Serial-JTAG keeps
across a software reset, which no transcript says.

**What it costs** — a host reading the board's words loses one line, and a
test that waits on log lines after a reboot under traffic is racy. Nothing
the update decides rides on it: the update channel and the hello are
reliable. The S3's update suites (`emu_s3_ota.rs`, `emu-cut-sweep.sh
--chip esp32s3`) therefore read the board's reliable words for "Y runs its
engine" (the newest hello after the loader's last line) and still check a
`[CORE]` line whenever it arrives.

**Close when** — the first session after a reset stops being reset by stale
host frames (or the cause is shown to be the emulator's queue, and fixed
there), and the S3 twins can read the `[CORE]` line as the C6's do.
