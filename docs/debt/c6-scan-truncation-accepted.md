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
config (one channel, 48-word halves / 60 µs deadlines) measures 0.49 % under
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

**What already self-serves** — since the runtime block plan (PR #276), the
RMT split is computed at driver init from the manifest's declared channel
count: a **1-channel manifest** (e.g. the C6 DevKitC profile) automatically
absorbs all four RMT blocks — a 192-word window with 96-word halves
(~120 µs deadlines, legacy-driver-class margin, wider than the measured
0.49 % config). No build flag, no config surface. What this entry carries is
only the **2-channel** tradeoff: a board that declares both outputs still
splits into 24-word halves and keeps the scan-truncation exposure above.

**What the virtual air (M4, 2026-09-08) does not change** — the emulator now
models ESP-NOW: a frame the blob hands the MAC is delivered, after a stated
latency, into every other machine's RX ring (`docs/adr/2026-09-08-virtual-air-claim-policy.md`).
That is **byte delivery on a perfect medium**, not a radio, and it cannot
reproduce this entry's truncation for the same reason it never claimed to:
the 28 % figure above is a *timing* property of the blob and the MAC — how
long the receive path is masked while a scan runs, measured against a 30 µs
refill deadline on real silicon — and the air has no scan, no PHY and no
interrupt-masking window to model. `validate.toml`'s `espnow-broadcast` set
says the same thing from the other side: what a replay holds equal is "every
**non-timing** field" (device ids, event numbers, kinds, byte counts, `gap`,
`len_ok`); nothing about when a frame arrived is claimed. A guest on
the air could run the exact `test_espnow`/`espnow-broadcast` image through a
scan loop and it would show no truncation whatsoever, which would not mean
the truncation is fixed — it would mean the air is not the instrument that
can see it. Closing this entry still wants the desk jig, on silicon, under
real scan traffic.

**Exit criteria** — reopens if OPC or E1.31 streaming (or any sustained
STA/UDP usage — the never-measured S4 scenario) enters the C6's product
path: measure S4 first, then revisit the 2-channel split via the recorded
matrix (plan dir `2026-08-01-1459-rmt-priority-hli`, p4 file + logs) — e.g.
an uneven `[1, 3, 0, 0]` plan or a different default. A future I2S/DMA
backend (no refill deadline at all) would retire this entirely.

**Log**

- 2026-08-01 — measured (P4 matrix), accepted at G1 with the reopen trigger
  named by Yona: "wifi load isn't a big concern for actual used installs...
  unless we start streaming opc or e131."
- 2026-08-02 — runtime block plan (PR #276): a 1-channel manifest now
  self-serves the wide (192-word) config at driver init; the carried debt
  narrows to the 2-channel split only.
