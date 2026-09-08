---
status: FIXED 2026-09-08 — `lp-emu/esp/lp-emu-esp32c6/src/periph/i2c_ana_mst.rs`
found: 2026-09-08      # M7 of the esp-emulator plan, on the ROM-up boot
area: lp-emu/esp/lp-emu-esp32c6/src/periph/accept.rs (`i2c_ana_mst`)
class: accept-block-too-coarse
related: [lp2025/2026-09-06-1001-esp-emulator/m7-rom-up-boot.md, docs/adr/2026-09-06-esp-soc-emulator-architecture.md]
---
# The analog I2C master is modelled as one data register, and the PHY reads it as a register file

**Symptom** — the ROM-up boot reaches the application and runs it to the
server loop, and three lines appear that neither silicon nor the direct-load
path prints:

```text
[INIT] Flash filesystem mounted
error: pll_cal exceeds 2ms!!!
error: pll_cal exceeds 2ms!!!
error: pll_cal exceeds 2ms!!!
[INIT] Creating output provider...
```

They come from the mask ROM's `wait_rfpll_cal_end` (`0x40005984`), which
polls an analog register through the PHY's ROM function table and gives up
after a fixed number of tries:

```text
400059a4:  li   a0, 20
400059a6:  jal  ets_delay_us
400059aa:  lw   a5, rom_phyFuns
400059b2:  lw   a5, 92(a5)
400059b8:  li   a0, 98            ; block 0x62
400059bc:  jalr a5                ; regi2c read, bit 1 of register 7
400059be:  bnez a0, done
400059c4:  addi a0, "error: pll_cal exceeds 2ms!!!"
```

**Cause** — `I2C_ANA_MST` is an accept-and-remember `RegFile`, so
`i2c_ctrl.data` (bits 16:23) reads back whatever was last written to
`i2c_ctrl` — one byte of state for the whole analog address space. A
`regi2c` transaction addresses a *particular* `{block, register}` pair
(`regi2c_read` writes `slave_addr` and `slave_reg_addr`, then reads `data`
back), so the model answers every analog register with the last value
written to any of them.

Direct load happens not to trip on this: the application's own `regi2c`
sequence leaves a value in `data` that makes the PLL poll succeed. That is
luck, not a model — and the ROM-up boot, whose ROM and bootloader have
already run their own `regi2c` traffic, does not get it.

**Why it is filed rather than fixed** — the fix is a real one (a
`{block, register} → byte` store behind the `i2c_ctrl` transaction, with the
PLL's calibration-done bit seeded), but it changes what every `regi2c` read
in every configuration answers, including the merged M4/M5/M6 gates that are
green on the current behaviour. Making that change inside M7 would put a
peripheral rewrite under a boot-path milestone's evidence. The three lines
are a **reported deviation** of the ROM-up path; the application still boots,
initialises the radio, and serves.

**What would close it** — model the transaction: on a write to `i2c_ctrl`
with `read_write` clear, look up `{slave_addr, slave_reg_addr}` in a byte map
and put the result in the `data` field; on a write with it set, store there.
Seed the one bit `wait_rfpll_cal_end` polls (block `0x62`, register 7, bit 1)
to 1, and cite this file. Then re-run every boot gate: a `regi2c` read that
changes value is a change to the clock and radio paths of every image.

**Where it belongs** — the same lab task as the accept-block reset-value
sweep (DD40 b), which is already M8's.

## Closed, 2026-09-08

Done as described. `I2C_ANA_MST` is a modelled peripheral now
(`periph/i2c_ana_mst.rs`): a write to `i2c_ctrl(n)` with `read_write` set
stores the `data` byte at `{slave_addr, slave_reg_addr}`, a write with it
clear looks the pair up and leaves the answer in the `data` field, and a
pair nobody has written answers 0 — the same answer the accept block gave,
but only for the register actually asked about. The two read overrides
(`busy` 0, `ana_conf0.cal_done` 1) came across unchanged.

`ANALOG_SEED` has exactly one entry, block `0x62` register 7 bit 1, and it
carries the disassembly that justifies it: `wait_rfpll_cal_end` calls
`rom_i2c_readReg_Mask(0x62, 1, 7, 1, 1)` and gives up after a hundred tries.

**The evidence.** `tests/rom_up_boot.rs::the_app_says_the_same_thing_on_both_boot_paths`
compares the app's whole console, line for line, between the ROM-up boot and
a direct load of the same ELF — 27 lines from `[INIT] Initializing board` to
`[RECOVERY] boot complete`. It passes. With `ANALOG_SEED` emptied, it fails
with **exactly** the three `pll_cal` lines inserted after `[INIT] Flash
filesystem mounted` and nothing else moved, which is the differential the
fix was asked for. Every other boot gate, the m3–m6 replays and every
memory-class figure are unchanged; the accept-block sweep landed in the same
PR and the two were measured separately.
