---
status: carried
since: 2026-09-08
logged: 2026-09-08
area: lp-emu-esp32c6 radio window (`periph/wifi_stub.rs`)
related:
  - lp-emu/esp/lp-emu-esp32c6/README.md ("The radio window", "The radio TX log")
  - docs/adr/2026-09-06-esp-soc-emulator-architecture.md
---
# A WiFi TX is armed on the emulated C6 and never completes

**Shape** — the emulated C6 runs esp-radio's blob far enough to build a real
frame and hand it to the MAC, and then stops. Nothing raises the `WIFI_MAC`
interrupt, so the blob's TX-completion path never runs, `esp_wifi_internal_tx`
never returns, and the guest makes no further progress.

**Measured 2026-09-08** on the `test_espnow` image
(`--no-default-features --features test_espnow,esp32c6`), M4 P0:

- The image prints its `radio ready` line, arms exactly **one** frame at
  ~1,036 ms of emulated time, and never prints a `tx simulated_button` line.
  106 bytes reach the host in a 200 ms run and 106 bytes in a 6 s run.
- `--break-at lmacTxDone --break-at ppProcTxDone --break-at trc_onPPTxDone
  --break-at esp_wifi_tx_done_cb` over 6 s: **none is ever entered** (exit 0,
  not 5). Breakpoints on `mac_tx_set_plcp0` and `lmacTxFrame` in the same
  configuration do stop, so the negative is a real one.
- After the arming write the guest never sleeps again — `idle skips (wfi)` is
  1,013 at 1.5 s and 1,013 at 6 s — retires ~160 M instructions/s, and issues
  **no MMIO of any kind**. It is spinning on RAM with the timer interrupt not
  being taken.

**Why it is carried** — closing it means *originating* an interrupt the
hardware would have originated, on a completion path nobody has observed:
which of interrupt sources 0–3 the MAC raises on TX done, which register the
ISR reads, which bits it expects, and which register clears them are all
undocumented and none of them can be learned from this emulator, because the
emulator is what is missing the interrupt. That is M4's virtual-air work
(P1/P2), and the roadmap deliberately did not plan it before the discovery.

**What is not blocked** — the frame itself. The bytes the blob hands the MAC
are complete and readable before the wedge, and `--tx-log` ships them: on the
image above, a 56-byte 802.11 vendor-specific action frame, broadcast to
broadcast, from the eFuse MAC, Espressif OUI `18:fe:34`, ESP-NOW element type
4. Anything that needs to see what the radio *would have sent* works today.

**What is blocked** — any image whose forward progress depends on a WiFi send
returning. `test_espnow` is one, and it is the only one in the tree; the
shipped image does not send at boot, which is why every existing C6 gate and
transcript is unaffected and none of them regressed when this was found.

**The evidence, in full**, including the register-by-register ledger of the
arming sequence and the descriptor read out of guest RAM:
`m4/discovery-air.md` in the 2026-09-08 C6 emulator rounding-out planning
directory.
