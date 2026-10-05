---
status: open
found: 2026-10-03      # how: hardware-walk — PR #943's classic desk sitting, step 0 (#884's owed lp-link walk), agent-run
area: fw-esp32v3 boot (`boot_firmware`'s raw `esp_println!` lines after io_task starts) × lp-link deframer (text outside frames)
class: unsynchronized-shared-artifact
related:
  - docs/defects/2026-08-02-serial-line-interleaving.md
  - docs/adr/2026-09-27-lp-link-one-comms-layer.md
  - lp2025/2026-09-28-2015-classic-uart-on-lp-link (hardware-walk-protocol.md, check (j))
  - lp2025/2026-10-02-1918-io-thread-other-boards (desk-classic.md)
---
# The classic loses its boot text when a host holds the link across a reset

**Symptom** — on the desk DOM-Z-102 (CH340K, `/dev/cu.wchusbserial112330`,
MAC `30:76:f5:ec:f6:34`), a boot that happens **with a host already on the
line** — a software `reboot` asked over the link (`lp-cli link capture
--request reboot`), or the reset `lp-cli upload` causes when it opens the
CH340 — almost always delivers the raw `[INIT]` boot text only up to
`[INIT] runtime started`. What `boot_firmware` prints after io_task starts
is lost, or arrives torn:

| image | boots checked with a host attached | what arrived after `[INIT] runtime started` |
|---|---:|---|
| main `d68791d96` | 2 (1 upload, 1 reboot) | **nothing** — `I/O task spawned`, `UART link task spawned`, `flash filesystem mounted`, `RMT ISR on APP core`, `heap region 3 live`, `JIT code region`, `fw-esp32 initialized … commit=` all missing, both times |
| step-1 `6a9d6e6ea` | 1 (upload) | nothing |
| PR #943 head `96dfc8cca` (product, diag and telemetry images) | 8 (6 uploads, 2 reboots) | 3 × nothing; 4 × one torn line and nothing between, e.g. `[INIT] I/O task spawned (uart0 921600 8N1, swi2 executor prio2, t[INIT] fw-esp32 initialized, starting server loop... proto=32 commit=96dfc8cca4d3 dirty=false`; **1 × every line whole** (the `ws281x_telemetry` image's upload) |

So it is timing-dependent, not deterministic, and it is not new with PR
#943: main loses more of the text than the branch does.

The same main image booted with **no** host (`classic-reset-and-capture.py
--reset run --baud 921600`, a raw read) prints all thirteen `[INIT]` lines
whole, and `lp-emu:esp32v3:t1` (lp-emu `c042102d9`, as built into `lp-cli` at main `d68791d96`; direct load,
memory FS) run with `lp-cli emu run --host-link --reboot-on-reset --request
'"reboot"'` delivers every line whole on both boots, with `[link] up` only
after `fw-esp32 initialized`. On silicon the host's `[link] reset
(PeerRestarted)` / `[link] up` land right after `runtime started` — the
link is back before the rest of the boot text has been written.

What is *not* affected: every log record (`[MEM]`, `[stack]`, `[JIT]`,
`[OUT] dump`, `[INFO] …`) in eleven captures and nine `link rtt` runs
arrived whole, and the lit `[OUT] dump` after a reboot matched the host
oracle to the byte. Log records are io_task's; only the raw boot text is
written by a second writer.

**Root cause (not yet confirmed)** — the 2026-08-02 interleaving fix made
io_task UART0's one writer *after boot*; during `boot_firmware`,
`esp_println!` still writes UART0's FIFO directly after io_task has
started. With a host already SYNing, the link answers during the boot (with
`io-thread` the link thread starts inside `boot_firmware` itself, which the
comment above `session_nonce` in `fw-esp32v3/src/main.rs` — "no host can
have a session before the link task runs, after `boot_firmware` returns" —
no longer holds; on main the session also comes up mid-boot on silicon).
A raw line that lands inside a link frame breaks the frame, and the lp-link
deframer's resync rule ("a frame that fails: stay inside a frame") then
swallows the text that follows as frame bytes until the next delimiter.
The emulator's boot is far shorter in host terms (memory FS, `t1` time), so
the session does not come back until the text is out.

**Cost** — the boot banner a host sees after a reset usually stops at
`runtime started`: no commit line, no flash/RMT/JIT init lines. The link itself
recovers (no app error, the new session's hello and every record arrive);
the host's `damaged` count after a reboot (1,467 on main, 1,469 on the
branch) is dominated by the ROM's 115,200-baud banner read at 921,600.

**Fix (open)** — candidates, for whoever takes it: route the post-io_task
`[INIT]` lines through io_task (as log records, or as whole text lines it
queues between frames, the way `[WS281X]` telemetry goes), or hold the
link's first SYN answer until `boot_firmware` has finished printing. A
regression test needs a boot with a host SYNing from the first byte and a
real-flash boot long enough for the session to beat the text (the emulator
reproduction with a merged image, `a_reboot_into_the_saved_project…`'s
shape, is unchecked).

Silicon only, agent-run desk sitting 2026-10-03; not graded by a transcript.
