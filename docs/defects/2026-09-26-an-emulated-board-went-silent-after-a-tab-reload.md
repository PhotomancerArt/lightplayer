---
status: open
found: 2026-09-26      # how: the learned-wire-dictionary G1 protocol run on the emulator (PR #835), run 2 of 5
area: lp-cli emu serve door × lp-emu-esp32c6 USB-Serial-JTAG coupling × fw-esp32c6 io_task (not yet separated)
class: fidelity
related:
  - docs/defects/2026-09-24-emulated-replug-leaves-the-old-byte-channel-open.md
  - docs/defects/2026-09-24-a-departed-page-kept-the-doors-boards.md
  - docs/adr/2026-09-25-learned-wire-dictionary.md
---
# An emulated board sent nothing for two minutes after a Studio tab reload

**Symptom** — Studio headless on `emu serve` (`lp-emu:esp32c6:t1`, lp-emu at
`6a4fed326`), editor lens at `?lens-pause-ms=75`, following the G1
hardware-sitting protocol (three cable pulls, two tab reloads). After the
**second** reload, Studio attached the board's port and sent `Hello` five
times, once a second (the wire tap shows ids 1–5 going to the board), and
the board sent **nothing at all** in reply: no Hello answer, no heartbeat,
neither packed nor JSON, for 120 s. Identify ended "unrecognized firmware".
The next emulated cable pull (detach, 3 s, attach) brought it straight back:
the new link identified in the same second.

**What it is not** — not a learned-table desync: a reader out of step names
every dropped frame (`wire: packed reply dropped …`), and there was no
frame to drop. Not the door's one-client rule: the new page's byte and
control clients attached normally (`TcpHost: client … attached`).

**How often** — once in five full protocol runs on the same image: two
packed runs with torn frames (one silent), two more packed runs with torn
frames and one packed run without (clean), one JSON run (`?wire=json`,
clean). The previous page had navigated away with a lens request in flight
(the tap shows request `…551` at 298.06 s unanswered before the reload at
300 s), which the clean runs may not have hit.

**Suspects, unranked** — (1) the board's "host not draining" latch: two
write timeouts after the old page left bump the link epoch and drop
protocol writes until the host is heard from; Studio's Hellos should have
cleared it (`note_host_active` on read), so either they never reached
`read_serial` or the probe grid never found the endpoint free; (2) the
emulated coupling's close/open ordering across a page reload
(`--usb-sj tcp: coupling: close: the port is not open` is logged at each
reload); (3) something in the firmware's write path waiting on a result that
never posts.

**Next step** — reproduce with the board's console (`--console-dir`) and the
`link_counters` stamps read out at the silence (`not_draining` /
`draining_again`), then the same run on `main`'s image to rule the learned
table in or out. Artefacts:
`~/.photomancer/planning/lp2025/2026-09-25-0006-learned-wire-dictionary/wire-tap/scripts/rec/g1-emu-2/`
(tap, journal, door log).
