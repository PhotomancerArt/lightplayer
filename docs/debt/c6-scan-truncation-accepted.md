---
status: carried
since: 2026-08-01
logged: 2026-08-01
area: fw-esp32c6 WS281x output (2-channel default config)
related:
  - docs/adr/2026-07-29-license-provenance-discipline.md
  - lp-fw/fw-esp32c6/README.md (WS281x section)
---
# The C6's 2-channel default truncates ~28 % of frames during WiFi scans

**Shape** — the shipped C6 default gives each of its two RMT TX channels one
48-word block (24-word ping-pong halves, 30 µs refill deadlines). Measured
2026-08-01 on the desk jig: under a continuous active-scan loop the
first-started channel truncates **28.0 %** of frames (guard-word trips;
bounded staleness, never corruption), the second 1.3 %. The alternative
config (`ws281x_2blocks`: one channel, 60 µs deadlines) measures 0.49 % under
the same load. Idle and ESP-NOW — the C6's actual shipping radio — measure
clean in both configs (worst: 1 trip in ~2,800 frames). The mechanism is
interrupt-to-service latency during the radio's masked windows, measured at
`Priority::max()`; RISC-V has no higher level, so no software priority fix
exists. Margin (bigger halves) is the only working software lever.

**Why it is acceptable now** — decided at the RMT-priority plan's G1
(2026-08-01): scan-class WiFi load is an editing-time concern, not a
deployed-install concern, and the second output is worth more than scan
robustness. The truncation is visible shimmer during scans, bounded to one
frame period per trip.

**Exit criteria** — reopens if OPC or E1.31 streaming (or any sustained
STA/UDP usage — the never-measured S4 scenario) enters the C6's product
path: measure S4 first, then revisit the default via the recorded matrix
(plan dir `2026-08-01-1459-rmt-priority-hli`, p4 file + logs). A future
I2S/DMA backend (no refill deadline at all) would retire this entirely.

**Log**

- 2026-08-01 — measured (P4 matrix), accepted at G1 with the reopen trigger
  named by Yona: "wifi load isn't a big concern for actual used installs...
  unless we start streaming opc or e131."
