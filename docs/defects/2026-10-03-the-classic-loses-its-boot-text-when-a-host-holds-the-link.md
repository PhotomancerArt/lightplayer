---
status: fixed
found: 2026-10-03      # how: hardware-walk — PR #943's classic desk sitting, step 0 (#884's owed lp-link walk), agent-run
fixed: this change     # write_link_text_mark() before boot_firmware's first [INIT] line
area: fw-esp32v3 boot (`boot_firmware`'s first raw `esp_println!` line) × lp-link deframer (the text mark, `TEXT_MARK` in `lp-base/lp-link/src/deframer.rs`)
class: two-clocks
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
oracle to the byte. Log records are io_task's, over the link, at the one
baud the host has always been correct about; only the raw boot text —
which spans the one baud change in the whole boot — is affected. (The
original write-up read this as "only the raw boot text is written by a
second writer"; the root cause below says why that reading was wrong.)

**Root cause (confirmed 2026-10-07, opus investigation)** — not a second
writer; the board sends the boot text whole. The ROM and the 2nd-stage
bootloader print at the classic's ROM default, 115,200 baud; a host (CH340K,
`lp-cli`'s `serial:*`, or the emulator's own `--host-link`) opens UART0 at
921,600, the app's own baud, and reads at that rate **for the whole boot**,
not just once the app has reconfigured the UART. A receiver reading a
115,200 line at 921,600 (8×) samples each real bit eight times, which the
investigation modelled as an ideal UART receiver (edge-triggered only while
idle, free-running for the rest of each frame once it locks on — real UART
hardware's own behaviour): seven of its eight "data-bit" samples land back
on the real start bit and only the eighth reaches the first real data bit,
so almost every misread byte is `0x00` or `0x80`. Fed through a real
`LinkConfig::uart()` host (`lp-base/lp-link`), that stream produced 1,470
bad frames (desk: 1,467) and left the deframer's frame state
(`lp-base/lp-link/src/deframer.rs`'s `push`, toggled by every `0x00`) as
likely "inside a frame" as not by the time the app's own, correctly-read
text starts. Inside a frame, that text is collected as a frame body instead
of being handed up as text — exactly PR #943's torn line, byte for byte,
and the cut at byte 531 (`max_frame`, `link.rs:1150-1151`) on captures long
enough to reach it. The 2026-08-02 interleaving fix (io_task as UART0's one
writer after boot) is not implicated: the emulator cannot show any of this,
because its UART delivers bytes clean whatever the baud is configured to
(`lp-emu-esp32v3/src/periph/uart.rs:398-399`) — the seam is the real
baud-rate mismatch between the ROM/bootloader's transmitter clock and the
host's receiver clock, not a race with a second writer.

**Cost** — the boot banner a host sees after a reset usually stops at
`runtime started`: no commit line, no flash/RMT/JIT init lines. The link itself
recovers (no app error, the new session's hello and every record arrive);
the host's `damaged` count after a reboot (1,467 on main, 1,469 on the
branch) is dominated by the ROM's 115,200-baud banner read at 921,600.

**Fix** — lp-link already has exactly the primitive this needs: its text
mark, a raw `0xFF` that resets a deframer's frame state unconditionally
however it was left (`deframer.rs`'s `TEXT_MARK` handling — the same byte
the panic path writes ahead of a crash report, `write_link_text_mark` in
`fw-esp32v3/src/recovery/panic_path.rs`). `boot_firmware` (`main.rs`) now
writes that same mark, reusing `write_link_text_mark` (made `pub(crate)`),
immediately before its own first `[INIT]` line — after the UART is already
at 921,600, so the mark and everything after it is read at the matching
baud whatever state the ROM/bootloader's misread garbage left the host's
deframer in. The bare-skeleton and radio-RAM-probe entry point
(`main.rs`'s other `fn main`, ~line 1527) does **not** get the mark: it
speaks no lp-link at all (no `UartLinkTransport`, just raw `esp_println!`
for a plain serial monitor), so there is no deframer there for a misread ROM
prefix to desync.

**Regression tests** — the emulator cannot reproduce the misread itself
(its UART is baud-blind), so the gate that gets to see it lives in
`lp-cli/tests/emu_v3_link_gates.rs`
(`the_text_mark_survives_a_baud_mismatched_host_reading_the_boot`): a small,
commented model of the baud mismatch (`misread_at_wrong_baud`) turns a real
ROM banner line into what a 921,600 host reads, then feeds `[misread ROM
bytes] + [app bytes]`, with and without the mark, to a real
`LinkConfig::uart()` link and asserts every `[INIT]` line arrives whole only
with the mark present. `lp-emu-esp32v3/tests/boot.rs`'s
`the_flash_status_spin_ends_and_a_blank_chip_has_no_partition_table` pins
the app's first UART0 bytes as the mark (`FF 0D 0A`) followed by
`[INIT] fw-esp32v3 boot`.

Silicon desk sitting 2026-10-03 (not graded by a transcript); root cause and
fix 2026-10-07, modelled against the emulator's own boot bytes re-read at
921,600 and verified against a real `lp-link` host.
