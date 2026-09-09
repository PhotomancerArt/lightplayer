---
status: FIXED same day by M5 P3 (PR #595), from the other side
found: 2026-09-08      # M7 of the esp-emulator plan, on the ROM-up boot log
area: lp-emu/esp/lp-emu-esp32c6/src/periph/usb_sj.rs (`IN_DRAIN_LATENCY_US`)
class: modeled-number-meets-a-new-path
related: [lp2025/2026-09-06-1001-esp-emulator/m7-rom-up-boot.md, lp2025/2026-09-06-1001-esp-emulator/m6-honest-usb-serial-jtag.md]
---
# The mask ROM's USB console drops characters the modelled drain latency delays

**Symptom** — on a ROM-up boot the two consoles carry different text. UART0
has the whole boot log; the USB-Serial-JTAG link has the same log with runs
of characters missing, most of them in the bootloader's partition table and
segment listing, where the output is densest:

```text
I (99) boot: ## Label            Usage          Type ST O15) boot:  0 nvs   …
```

The observation stream (`--usb-sj-tried`) is **empty**, so nothing was
dropped by the model. The guest never wrote those bytes.

**Cause** — the mask ROM's console drops rather than waits, on one of its two
paths. `usb_serial_device_tx_one_char` (`0x40022856`) reads
`ep1_conf.serial_in_ep_data_free` and, when the endpoint is not free and it
has not yet marked the link connected, returns without writing:

```text
40022862:  lui  a4, 0x6000f
40022866:  lw   a5, 4(a4)        ; ep1_conf
40022868:  andi a5, a5, 2        ; serial_in_ep_data_free
4002286a:  beqz a5, 40022872     ; -> return 0, the character is gone
```

The retry path it takes once connected waits 50 × 100 µs and then *clears*
the connected flag (`0x40022890`), which puts the next character back on the
dropping path.

`IN_DRAIN_LATENCY_US = 100` is the model's answer to "how long after
`wr_done` has a draining host taken the packet", and M6 graded it **modeled**
— "sub-millisecond, well under every timeout the firmware uses". Every
firmware writer M6 measured waits. The mask ROM is the first writer that does
not.

**What silicon says** — the committed `boot-idle-flash` capture is a complete
boot log over this very link, from this very ROM, with a draining host. So
silicon's host takes packets fast enough that the ROM's dropping path never
fires, which is an upper bound on the real latency that M6 did not have.

**Why the number was not changed here** — it is an input to every M6 gate and
to M6 P5's byte-equal walk, and moving it because a new path is unhappy is
tuning. What M7 does instead is **gate the boot log on UART0**, where the ROM
writes the same bytes with no host model in the way, and say so in the test.

**What closed it** — not this number, and not a desk sitting. M5 P3 (PR #595)
made the IN FIFO **auto-commit when it fills**, which is the fidelity fix a
different payload needed: a log line longer than 64 bytes used to sit in an
uncommitted FIFO until the writer's next `wr_done`. With it, the ROM's
64-byte-at-a-time console never finds the endpoint stuck for a whole drain
latency, and on a rebase onto that change the two consoles carry **identical
bytes** across the entire boot window — 32 lines each, checked by
`tests/rom_up_boot.rs`, which now reads the USB link (silicon's own) and
asserts UART0 matches it.

`IN_DRAIN_LATENCY_US` is untouched and still *modeled*. The bound silicon's
capture puts on it stands, and is worth keeping written down: a complete boot
log over that link from this ROM says the real latency is short enough that
the ROM's dropping path never fires. If the number is ever measured, re-run
every M6 gate and M6 P5's walk against it — it decides when a firmware
writer's `write` future completes.
