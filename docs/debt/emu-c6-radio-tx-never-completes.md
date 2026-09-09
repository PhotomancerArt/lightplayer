---
status: carried
since: 2026-09-08
logged: 2026-09-08
area: lp-emu-esp32c6 radio window (`periph/wifi_stub.rs`)
related:
  - lp-emu/esp/lp-emu-esp32c6/README.md ("The radio window", "The radio TX log", "The air, and the lockstep pair")
  - lp-emu/esp/lp-emu-esp32c6/src/periph/wifi_stub.rs (RADIO_INT_SOURCES, MAC_INT_EVENT_OFFSET)
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

**Narrowed 2026-09-08, M4 P1.** Three of the four unknowns above are now
answered by observation of *our own* registers on the same image. The fourth
— which bits mean "your frame went out" — is not, and the debt stays open on
it alone.

- **Which source.** esp-radio's `os_adapter_chip_specific::set_isr` routes
  sources **0 (`WIFI_MAC`) and 2 (`WIFI_PWR`)** to CPU interrupt 16 at
  5.6 ms, `cyc=5619236 pc=0x4202c9d8 W4 INTERRUPT_CORE0+0x000
  core_0_intr_map0 = 0x00000010`. CPU interrupt 16 was already enabled at
  priority 1 by esp-hal's `_setup_interrupts` (`mxint16_pri = 1`,
  `mxint_enable = 0x00010000`) and carries the live esp-rtos tick, so a
  raised source 0 **is** taken. Sources 1 (`WIFI_MAC_NMI`) and 3
  (`WIFI_BB`) are left at the default of 31 and never claimed.
- **Which register the ISR reads.** Raising source 0 after the go strobe
  enters the blob's ISR, which reads `WIFI_MAC+0x4c48` through
  `hal_mac_interrupt_get_event` (`cyc=165843231 pc=0x4080d232`), then
  `WIFI_MAC+0x4c34` (`hal_mac_interrupt_get_bsscolor`) and
  `WIFI_PWR+0x37b0` (`hal_pwr_interrupt_get_event`).
- **Which register clears it.** `WIFI_MAC+0x4c4c`, written with exactly the
  bits just read (`cyc=165843260 pc=0x4080d23c W4 WIFI_MAC+0x4c4c =
  0x00001000`) — a write-one-to-clear. `WIFI_PWR+0x37b4` is its twin.
  `mac_txrx_init` writes `+0x4c4c = 0xffffffff` at init, which is the
  second reading that says it is a clear. A block that raises the line
  without honouring the clear re-enters the ISR every 464 cycles forever.
- **Which bits it expects: still unknown.** All 32 bits of
  `WIFI_MAC+0x4c48` and all 32 of `WIFI_PWR+0x37b0` were tried **one at a
  time**, and with all bits set at once, against the stated oracle
  (`--break-at lmacTxDone --break-at ppProcTxDone --break-at
  trc_onPPTxDone`, and the guest's own `tx simulated_button … event=N`
  line). None reaches the completion path: the ISR consumes the event,
  clears it, returns, and the guest goes back to the same spin — with an
  **identical instruction count (15,871,852 at 1,100 ms) for all 64
  candidates**, so the event word's value does not reach a dispatch at all.

**Why it is still carried** — closing it means *originating* an event the
blob's software accepts on a path nobody has observed, and P1 stopped rather
than guessing past the evidence. The specific question a bounded
interpretation pass or a silicon capture would answer is now one line:

> The blob's ISR (entered through the handler `set_isr` registered at
> `0x4202c9d8` for source 0, reached at `hal_mac_interrupt_get_event`,
> `0x4080d22e`) reads `WIFI_MAC+0x4c48` and `WIFI_PWR+0x37b0`, clears both,
> and returns without entering `lmacTxDone` (`0x40803704`) for any value of
> either word. **What else must be true — which other register, or which
> RAM state — before the ISR dispatches a TX completion?**

Note the shape of that question: it is no longer "which bit", because no bit
of either word is sufficient. Something outside those two words gates the
dispatch. M4 P3's two-board silicon capture is the oracle that would settle
it, and `d1-desk-batch.md` step 3 is where it is spent.

**What M4 P1 shipped instead** — the `Air`, the lockstep pair runner and the
socket framing, with the receiving end **counting and logging** rather than
delivering. No completion is originated, no interrupt is raised, and a
machine that is not in an air is byte-for-byte the machine that came before
the air existed. The constants above are recorded in `periph/wifi_stub.rs`
(`RADIO_INT_SOURCES`, `MAC_INT_EVENT_OFFSET`, `MAC_INT_CLEAR_OFFSET`,
`PWR_INT_EVENT_OFFSET`, `PWR_INT_CLEAR_OFFSET`) with the trace lines that
named each, and nothing in the block acts on them.

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
