---
status: retired
since: 2026-09-08
logged: 2026-09-08
retired: 2026-09-08
area: lp-emu-esp32c6 radio window (`periph/wifi_stub.rs`)
related:
  - lp-emu/esp/lp-emu-esp32c6/README.md ("The radio window", "The radio TX log", "The air, and the lockstep pair")
  - lp-emu/esp/lp-emu-esp32c6/src/periph/wifi_stub.rs (TX_DONE_INT_EVENT_BITS, TXQ_STATE_OFFSET, MAC_INT_EVENT_OFFSET)
  - lp-emu/esp/lp-emu-esp32c6/tests/air_delivery.rs (the U1 sweep, and the gate)
  - docs/adr/2026-09-06-esp-soc-emulator-architecture.md
---
# A WiFi TX is armed on the emulated C6 and never completes

> **RETIRED 2026-09-08 by M4 U1.** A TX completes. The guest's own
> application prints `[test_espnow] tx simulated_button device= event=1`
> and, a second later, `event=2`; a staggered pair now holds a two-way
> conversation. **What it took is the last section of this file**, and the
> history below it is kept unedited because the shape of the negative — and
> why two phases read it as stronger than it was — is the useful part.

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

**Still open after M4 P2, and one claim above needs narrowing.** P2 built the
RX half — a frame the air carries is written into the receiving guest's own
descriptor ring, source 0 is raised with bit 14 in the MAC's event word, and
the receiving application prints `[test_espnow] rx …`. Three things follow
for this debt:

- **The TX completion is untouched and still open.** Nothing here originates
  one, and the RX raise makes no difference to it.
- **A working RX path is not what the TX was waiting for.** In the pair run
  the receiving machine prints its `rx` line, goes on to arm its own frame,
  and wedges exactly as before — it never prints a `tx` line either.
- **P1's "the event word's value never reaches a dispatch" was measured with
  an empty RX ring, and only holds there.** With a frame in a descriptor the
  32 candidates separate into eight distinct instruction counts and bit 14
  reaches the blob's RX path where no other bit does. That does not reopen
  the TX question — it *sharpens* the method: a bit sweep against a machine
  that has nothing to find measures the experiment, not the mechanism. If a
  future pass sweeps for the TX bit, it should do so with whatever state a
  real TX completion would find, not with the machine as it sits.

**What is not blocked** — the frame itself. The bytes the blob hands the MAC
are complete and readable before the wedge, and `--tx-log` ships them: on the
image above, a 56-byte 802.11 vendor-specific action frame, broadcast to
broadcast, from the eFuse MAC, Espressif OUI `18:fe:34`, ESP-NOW element type
4. Anything that needs to see what the radio *would have sent* works today.

**What is blocked** — any image whose forward progress depends on a WiFi send
returning. `test_espnow` is one, and it is the only one in the tree; the
shipped image does not send at boot, which is why every existing C6 gate and
transcript is unaffected and none of them regressed when this was found.

It is also what bounds a **pair**: each machine arms one frame and stops, so
a two-board run exchanges exactly one frame in one direction, and only if the
machines are staggered (`lockstep::Lockstep::stagger`) so that the receiver
is not already wedged when the frame lands. Closing this debt is what would
turn that into a conversation.

**The evidence, in full**, including the register-by-register ledger of the
arming sequence and the descriptor read out of guest RAM:
`m4/discovery-air.md` in the 2026-09-08 C6 emulator rounding-out planning
directory.

---

## Retired 2026-09-08 by M4 U1 — what it took

**The answer to the question above.** The ISR *does* dispatch on a bit of
`WIFI_MAC+0x4c48`, and it then asks a **second** register whose existence
neither P1 nor P2 knew about:

| step | what | evidence |
|---|---|---|
| 1 | **Event bit 7** of `WIFI_MAC+0x4c48` sends the ISR into `hal_mac_get_txq_state` (`0x40806382`) | `cyc=176010445 pc=0x408063f6 R4 WIFI_MAC+0x4cb8 = 0x00000001  hal_mac_get_txq_state+0x74`. Bits **8** and **19** reach the same function by other routes; no bit of `+0x4c34`, `WIFI_PWR+0x37b0` or `+0x37ac` reaches it at all |
| 2 | **`WIFI_MAC+0x4cb8` bit 0** is what it wants. Answering 0 returns from the ISR — the negative P1 and P2 both measured; answering bit 0 enters **`lmacTxDone` (`0x40803704`)** | `--break-at lmacTxDone` stops (exit 5) for `{bit 7, +0x4cb8 = 1}` and does not for `{bit 7, +0x4cb8 = 0}`. `+0x4cb8 = 0x10` does **not** complete, so it is not "any non-zero value" |
| 3 | Past it, `hal_mac_get_txq_complete+0x42` reads `WIFI_MAC+0x54e0`, which an accept-and-remember 0 answers fine | `cyc=176010453 pc=0x4000bcee R4 WIFI_MAC+0x54e0 = 0x00000000` |
| 4 | The guest **clears the queue state itself**, through `+0x4cb4`, in the same write-one-to-clear shape as `+0x4c4c` | `cyc=176010642 R4 WIFI_MAC+0x4cb4 = 0x00000000  hal_mac_clr_txq_state+0x36` then `cyc=176010645 W4 WIFI_MAC+0x4cb4 = 0x00000001  +0x40` |
| 5 | The guest's own application prints the line this debt was about, and goes round its loop | `[test_espnow] tx simulated_button device= event=1`, then `event=2` a second later. `idle skips (wfi)` 1,013 → 2,390 and 159 M instructions at 2 s → 6.08 M: the spin is gone, not hidden |

**Why two phases read the negative as stronger than it was.** P1 and P2 both
discriminated candidates by the run's **instruction count**. The wedged guest
retires one instruction per cycle inside `send_channel`'s spin, so a run
bounded by a cycle deadline retires the *same* number whatever the ISR did
with its tens of instructions — "identical instruction count for all 64
candidates" measured the deadline, not the mechanism. Counting the
**radio-window accesses each raise produced** instead separates 132
candidates into **eight paths**, and three of them are the TX ones. P2's own
lesson ("a bit sweep against a machine that has nothing to find measures the
experiment") was right and did not go far enough: the *instrument* was also
measuring the wrong thing.

**And the wedge was never in the blob.** The frame-pointer chain at 1,100 ms,
1,500 ms and 2,000 ms is identical and it is our own firmware —
`Esp32EspNowRadioDevice::send_channel+0x1fe`, i.e. `sender.send(..)` returned
`Ok` and `.wait()` was spinning for a send callback. esp-radio's `esp_now`
had accepted the frame; only the completion was missing. The spin never
yields, and the RTOS is tickless, so **nothing else on the machine ran
either** — an unfiltered trace shows zero accesses of any kind between
1,100 ms and 1,400 ms — which is why only an interrupt could unwedge it.

**What is modelled, and what is chosen.** All of it is `modeled`
([`AIR_GRADES`]) and no silicon has been watched:

- **Observed:** that bit 7 dispatches, that `+0x4cb8` gates it, that `+0x4cb4`
  is its clear, that `+0x54e0` is read next.
- **Chosen:** that the completing slot is `+0x4cb8` **bit 0** (the blob
  programmed one slot and nothing distinguishes them), and the **672 µs**
  delay between the go strobe and the completion — the same air time
  `lockstep::DEFAULT_LATENCY_US` was chosen from, stated rather than measured.
  It is not zero on purpose: a guest must not see its own frame complete in
  the store that armed it.

**The off switch is kept.** Only a machine that asked for radio behaviour —
`WifiStub::arm_air` / `arm_tx_completion` — completes a TX. A plain run of
the same image still retires **79,871,852** instructions at 1,500 ms and
still prints no `tx` line, so every existing C6 gate and transcript stands
without being re-recorded.

**What this unblocks.** M4 P0's U2, U4 and U7 all had one blocker — "make one
TX complete" — and it is gone: `test_espnow` now sends every second, so a run
long enough gives frames of one length from one boot, and P3's payload gives
the differing lengths U2 wants. A staggered pair is now a **conversation**:
both guests print the other's frame and their own `tx` line
(`tests/air_delivery.rs::the_pair_hears_itself`,
`the_symmetric_pair_talks_both_ways`).

**What is still not claimed.** That the silicon's TX-done event is bit 7, that
its queue-state register is `+0x4cb8` bit 0, or that a real completion arrives
672 µs after the strobe. The blob accepts this and behaves; a capture from one
of the desk C6s is what would turn any of it from `modeled` into `measured`,
and M4 P3's two-board capture is still where that is spent.
