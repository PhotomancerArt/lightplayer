---
status: fixed
found: 2026-10-07     # ci
fixed: this change
area: lp-cli/tests/emu_uart_link.rs (the classic's UART fault soak)
class: wait-on-a-lossy-signal
related:
  - lp2025/2026-10-06-1945-ci-director
  - fd6f4444e (added the compile wait this replaces)
---
# The UART soak waited on a log line its own faults lose

**Symptom** — `an_upload_through_kilobyte_runs_of_uart_loss_sees_no_app_errors`
took about 12 minutes in CI's "Emulator ESP32v3 (x64)" job and pushed it past
its 35-minute timeout: PR #1008's run 37578272693 (`emu_uart_link` finished in
799.03 s) and PR #1015's run 37580175307 (730.30 s). Main's runs finished the
whole file in about 38 s. #1008 changed the test profile (thin LTO) and #1015
did not, so the profile was a coincidence. The test passed every time, with 0
app errors.

**Root cause** — two things, both deterministic in emulated time. The host
side never reads a wall clock: `EmuLinkHost` steps the board and the host's
link off `board.micros()`.

1. *Which path the soak takes depends on the image's bytes.* The fault
   injector damages the UART in fixed 64-byte windows. A `run` swallows the
   next 16 windows, whenever those bytes cross. So any change in what the
   board sends moves every later fault. The hello carries the build's version,
   which is `2026.10.06-19` on a main build and the short sha (`e41c6349e`,
   `c1bc39ab5`) on a PR build. All three images' `lp-fw` trees are identical
   (`37253fe3873b`), and 67 bytes differ between the ELFs. Every main image
   takes one path: 6.3 s emulated, 5 runs of loss. Every current PR image
   takes another: 27.5 s emulated, 12 runs. Run against the same fat-LTO
   `--release` test binary on a desk, CI's own images reproduce those counters
   exactly. The extra emulated seconds cost almost nothing, because the board
   sits idle in `waiti` (~9 M instructions per emulated second) while a run of
   loss eats its resends. The five rounds took ~6 s of wall on the slow path.
2. *After the soak, a wait keyed on a best-effort log line.* fd6f4444e made
   the test wait for the last compile to end before watching for the board's
   heartbeat. It decided that by looking for `[shader-node] compilation
   succeeded` after the last `compilation starting` in the console. Those are
   `log` records, which go on lp-link channel 2. That channel is best effort
   and nothing resends it. On the slow path the injector dropped the
   `succeeded` record (a desk run with the fix still saw the old predicate
   true *after* the heartbeat had arrived). So the loop ran to its ceiling,
   the 120 s answer budget. All that time the board had a project loaded and
   was rendering flat out, at ~234 M instructions per emulated second. That is
   ~28 G emulated instructions, about 12 minutes on a CI runner and 18–27
   minutes on a loaded desk.

The cost does not scale with host speed. A ~20 % slower test binary does not
make a test 100× longer. A lost log line turns the wait's ceiling into its
duration, and that happens only on the byte path where the line is lost.

**Fix** — the post-soak wait now waits only for the heartbeat, with the answer
budget as its ceiling instead of a 6 s window. The heartbeat is a proto
message (channel 1), which is resent until it lands. The compile still holds
the server loop, and the heartbeat comes once the compile is done. The
`compiling()` predicate is gone. The assertions are unchanged. On a desk
(load ~180, M2 Max, fat-LTO `--release`, CI's images): the two slow-path
images now pass in 37–82 s (`e41c6349e`'s took 1091 s and 1597 s before), with the heartbeat 2.6 s emulated
after the last round. Main's image passes in 89 s against 22–32 s before,
because the wait now runs to the heartbeat. The emulated counters are the
same on both paths.

**Regression coverage** — the soak itself. With the fix it costs at most one
compile plus one heartbeat period after the rounds, whichever path the image
takes. Nothing pins the wall time, and nothing should: emulated time is the
only clock a test may gate on.

**Lesson** — "Wait for the board's words" is the rule for walks. A board's
words on a lossy channel are only evidence when they arrive, though, and a
fault soak is built to make some of them not arrive. A wait whose exit is a
log line has two outcomes: its predicate holds, or its ceiling expires. When
the ceiling is generous (120 s here) and the board is busy, the second
outcome is a CI timeout, not a failure. Key waits on a reliable message (a
reply, a heartbeat), or on an emulator-side fact the link cannot lose.
Separately, a fault soak's path is chaotic in every byte the board sends, so
its emulated length differs between main and PR images of the same source.
Judge such a test by its counters, never by one run's duration.
