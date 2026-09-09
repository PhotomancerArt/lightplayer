---
status: OPEN — documented and pinned by a gate; not fixed, and the fix is a director call
found: 2026-09-09      # M3 P2 of the c6-emulator-rounding-out roadmap, on the stub half of G2-2
area: lp-emu/esp/lp-emu-esp32c6/src/periph/ (the C6 boot set's block list)
class: unmapped-block-on-a-new-path
related: [lp2025/2026-09-08-0839-c6-emulator-rounding-out/m3/p2-espflash-over-socket-rom-up-hello.md, lp2025/2026-09-08-0838-emulator-plan-two-web-serial-shim/plan.md, docs/adr/2026-09-06-esp-soc-emulator-architecture.md]
---
# Espressif's flasher stub reads `I2C0.scl_high_period` before attaching the flash

**Symptom** — `espflash`'s default (stub) path over the emulated download
console uploads and runs, then ends the run 19.5 M cycles in:

```text
StrictViolation { cycle: 19524688, pc: 1082133018, address: 1610629176,
                  width: Word, access: Read, in_mmio_window: true, grade: None }
```

`pc` is `0x4080_0a1a` and `address` is `0x6000_4038`.

**Cause** — `pc` is inside the stub espflash just uploaded, not inside the
mask ROM. Disassembling espflash 3.3.0's own
`resources/stubs/stub_flasher_32c6.toml` (`text_start = 0x4080_0000`):

```text
40800a16:  lui    a0,0x60004
40800a1a:  c.lw   a0,56(a0)      # 0x6000_4038      <-- refused here
40800a1c:  c.andi a0,28          #   bits [4:2]
40800a1e:  c.addi a0,-8          #   == 2 ?
40800a20:  sltiu  a0,a0,1
40800a24:  c.li   a1,0
40800a26:  auipc  ra,0xff7ff
40800a2a:  jalr   ra,1974(ra)    # 0x4000_01dc <__call_spi_flash_attach>
```

`0x6000_4000` is **I2C0** (`esp32c6-0.23.2/src/lib.rs:431`) and `+0x38` is
`scl_high_period` — "Configures the high level width of SCL". The stub reads
three bits of an I2C clock-timing register and passes the resulting boolean
to the mask ROM's `spi_flash_attach` (`0x4000_01dc` → `0x4002_4cfa`).

This machine's C6 boot set does not map I2C0 at all — no firmware on this
chip has ever driven it — so `--strict-bus` refuses the read, which is the
emulator being honest rather than the stub being wrong.

**How much it costs, measured** — exactly one access.
`tests/flash_over_socket.rs::the_stub_writes_the_image_once_the_unmapped_block_reads_zero`
runs the same stub with the bus's refusal off, so an unclaimed address reads
0 and is counted, and asserts:

- `unmapped_reads() == 1` and `unmapped_writes() == 0` for a whole
  upload → erase → program run, and
- the 64 KiB the stub then wrote through the ROM's SPI1 routines is
  byte-identical to the image.

So the stub path is **one unmapped block away from working**, and that block
is touched once.

**Why it is not fixed here** — three reasons, in order of weight.

1. **Nobody has measured what the register reads on a part.** Zero is what
   the PAC's reset value says and what an untouched I2C0 would hold, and it
   makes the stub's `(v>>2)&7 == 2` test false — but "the model returns 0
   because the PAC's reset value is 0" is a `modeled` claim, and mapping the
   block would be the machine asserting it. Running permissively and counting
   is the same information without the assertion.
2. **`periph/accept.rs` is fenced** for every phase of this roadmap. A reset
   value that belongs in an accept block is a defect entry, which is what
   this is.
3. **`--no-stub` works**, so nothing the milestone needs is blocked. G2-2 asks
   for both paths recorded, and this is the record.

**What a fix would look like** — an `accept::i2c0()` block in the boot set at
`0x6000_4000`, seeded from the PAC's reset values the way the 2026-09-08 sweep
seeded the others (see
`docs/defects/2026-09-07-accept-blocks-carry-only-the-reset-values-a-boot-needed.md`),
and `I2C0` added to the strict-grade scope of the stub test with an honest
`modeled` grade on `scl_high_period` until somebody reads it off a board. That
is a director call, not this phase's: it adds a peripheral to the boot set for
one register on a path no product code takes.

**Why plan two cares** (its OQ1) — the question there is whether esptool-js in
a browser can skip the stub. The answer this gives is that it does not have
to: the stub's *own code* runs correctly on the modelled hart for nineteen
million cycles and writes correct bytes, and the single thing between it and a
clean run is one unmapped I2C register read. Both halves of that are useful —
the stub is not a wall, and `--no-stub` is available if the browser half would
rather not carry it.
