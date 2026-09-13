# `lp-emu-esp-common` — the Espressif SoC substrate

Everything an ESP machine emulator needs that is **not** a chip fact: the
bus, the MMIO decode table, the peripheral model, the accept-and-remember
register file, the bus trace, the host byte streams, and an ELF
program-header view.

MIT, inside the `lp-emu/` fence (`just lint-emu-fence`). See `../../README.md`
and `docs/adr/2026-09-06-lp-emu-home-and-mit-fence.md`.

## The neutrality rule

**This crate contains no chip numbers.** Not one base address, not one
register offset, not one interrupt source number, not one clock rate. A chip
crate — `lp-emu-esp32c6` first, whatever follows it beside — builds a
`SocBus` by registering its own RAM regions, MMIO windows and peripherals,
and everything here works the same for a C6, an S3 and a classic ESP32.

The same rule sends the generated register-name tables to the chip crate: a
register layout is chip-family data. `regnames` holds the type and the
lookup, never a table. (The two tables under `tests/regs/` are the
generator's proof, not the crate's data; see below.)

## Engines and views

**An engine is behaviour without a register map.** A peripheral block on an
Espressif part is two things wearing one name: what the hardware *does* — a
FIFO pair draining at a baud, a counter reaching an alarm, a flash command
engine walking its phases — and *where the guest pokes it*. The first is the
same IP across three generations of the part. The second is different on
every one of them.

So: **behaviour, scheduled events and host streams may live in an engine. A
register offset, a bit position, a reset value, an interrupt source number
and a `RegGrade` may not.** An engine names its events (`rx_overflow`,
`tx_done`); the chip's **view** maps those names onto the bit positions its
PAC declares, seeds its own reset values, publishes its own grades, and owns
the `Peripheral` impl. The neutrality rule is not weakened by engines —
engines are how it is kept once a second chip arrives.

Engines exist **only where the win is real**: a second chip's view would
otherwise re-implement scheduled behaviour with host-stream or fabric
coupling. A shared struct with no scheduling in it is not a win, and a
forced abstraction over two generations of different IP is a cost. The RMT
is the worked example of a **no**: the classic's eight symmetric channels
and the C6's split TX/RX register families are different enough that a
shared engine would be a fiction. GPIO is a second no, for the opposite
reason: its shared part already exists and is called `Fabric`.

An engine is a plain struct, not a trait: the view owns one **by value**,
calls methods on it and passes its `BusCx` through. No generics, no
callbacks, no trait objects on a hot path — and every scheduling decision
stays visible at the call site. An engine never packs an `EventId` either;
it does not know its peripheral index, and a renumbering would change *when*
events fire.

| engine | why it earned one |
|---|---|
| `engine::uart` | Two writers share a real UART — a mask ROM's direct FIFO store and an async driver filling it and awaiting a threshold — behind a shifter draining at the programmed baud in emulated cycles, a receive timeout, threshold levels, and a `HostSinks` byte stream with a scheduled source poll. Scheduled behaviour with host-stream coupling, and on the classic ESP32 the UART is the *only* host link |
| `engine::spi_flash` | The flash side of a controller's command word — the `usr` transaction's operations, the WIP/WEL latch a mask ROM spins on, the SPI-NOR command set — and the NOR chip itself: program is `&=` so a double-write without an erase shows up, erase is the only way back to `0xff`, the JEDEC capacity byte comes from the same length the ROM's `chip_size` word does, and `--flash` / `--flash-copy` / blank are three distinct persistence policies. Every part in this emulator boots off SPI-NOR, and a second view would otherwise re-derive file-backed persistence exactly or its transcripts would mean nothing |
| `engine::timg` | Counters at a rate with alarms, auto-reload, the load/update pair, and the watchdog's write-protect gate. The classic ESP32 has no SYSTIMER: TIMG0 is its rtos tick, its 1 ms io pacer **and** its `Instant::now()`. Three scheduled counters feeding `IrqLines`, and the engine takes the count as a parameter so the classic's third one needs no new type |
| `engine::sha` | **A different justification from the rest — see below.** The SHA-1 and SHA-256 compression functions, `K256`, the three initial vectors and the eight-word state. Not shared scheduled behaviour: one implementation of a standard algorithm that three chips must agree on bit for bit, because an ESP-IDF second-stage bootloader will not run an image whose hash does not match the digest `esptool` appended. Cheap to share (a pure function, no `BusCx`, no scheduler), expensive to get wrong twice. SHA-384/512 are **not** here: the classic has them and the C6 does not, and they arrive when a boot actually needs them |

Two things a reader might look for in `engine::timg` and not find. The
**RTC calibration** stayed in the C6's view: its shape is generic but its
content is two chip clock rates, it has exactly one consumer, and the
classic's RTC path differs enough that a shared version would be a fiction
before it was a saving — the classic's own view may revisit that in M3. And
the **MWDT's expiry** is not modelled at all, here or in the view: arming
one leaves a trace note saying so, and no stage ever fires. A watchdog that
started firing would be a behaviour change wearing a refactor's clothes.

**What stayed chip-specific, and why.** The C6's `periph/spi0.rs` did *not*
become a view: four of its registers are live and three of those drive the
cache MMU (`Cache_MMU_Init`, `Cache_MSPI_MMU_Set`, `MMU_Get_Page_Mode`), which
is a chip's own address translation and not flash behaviour. Its only overlap
with SPI1 is the self-clearing trigger mask, fifteen lines that name bit
positions — and bit positions are exactly what an engine may not hold. The
cache MMU, the cache-fill path and the flash window stay in the chip crate for
the same reason. So does every trigger-bit number: they happen to agree across
two generations of this IP, and this crate does not encode coincidences.

**`engine::sha` is the one engine whose justification is not the test above,
and it is not a precedent.** A block compression schedules nothing and touches
no host stream, so on the rule as written it does not qualify. It is extracted
anyway because bit-exactness across chips is a different kind of win and a
real one: a second hand-transcribed copy of sixty-four round constants and a
round function is duplication that fails only at the last mile of a boot,
where it is most expensive to diagnose. Do not cite it when arguing for
extracting a struct two chips happen to share — the argument has to be that
*this* algorithm has an external standard three views must agree with, not
that two views have similar fields.

### The USB-Serial-JTAG finding (M2, for M6)

The C6's `usb_sj` block is offset-identical to the S3's for the whole range
the S3 has (`EP1` @0x00 through `FRAM_NUM` @0x24, the interrupt quad at
0x08–0x14 in the same order), and the C6's extra registers are a superset
rather than a conflict — so M6 should **reuse the C6's file** rather than
re-derive it. Where such a file lives is a placement question, not an
engine/view one, and it is M6's to execute: this crate is not the home,
because its rule forbids register offsets and that block is nothing but
offsets.

## The layering

```text
  machine (chip crate)   reset, memory map, interrupt matrix, ROM, CLI
        |
  SocBus                 RAM regions + MMIO decode + watchpoints
   |        |            + the unmapped policy + sideband
   |    Peripheral       read / write / on_event against a BusCx
   |        |            — a chip's register VIEW of one block
   |        |--- RegFile accept-and-remember, with a table of exceptions
   |        |--- engine  what the block DOES, with no register map
   |
  Trace                  every MMIO access, with the PC, plus SPIN
  HostSinks              where a UART's bytes actually go
  Fabric                 pads, signals and edges — where an output goes
  Scheduler              (in lp-emu-core) guest time — plan PD5
```

### `bus` — regions and decode

RAM regions are sorted by base, looked up by binary search behind a
"last region hit" cache, and carry `exec` / `writable` flags: instruction
fetch comes only from an `exec` region (never from MMIO — a jump into
peripheral space is a wild branch, and answering it with a register's value
turns a crash into a puzzle), and a read-only region models ROM and the
flash cache window while `load_image` still places bytes into it from the
host side.

The bus serves **two fetch shapes**, because it serves two instruction sets.
`fetch_instruction` is the word-assembling one: fixed-width and compressed
RV32, which means a two-byte alignment rule and a `u32` built 4-bytes-then-2.
`fetch_bytes` is the byte-granular one: Xtensa's instructions are 2 or 3 bytes
and start at **any** alignment, so there is no word to return and no alignment
rule to apply — the caller gets up to three bytes and a count. Both obey the
same two rules. Neither ever routes to MMIO (the default `Bus::fetch_bytes` in
`lp-emu-core` does, because it is built from `read_u8`, which is why `SocBus`
overrides it: a fetch that walked into UART0 would pop its FIFO). And both are
bounded by the **region's** own end rather than the arena's, even though the
arena is one flat allocation in which two adjacent regions are contiguous
bytes. The straddle rule follows: *the count says how many bytes are really
there; a decoder that needs more than it got has run off the end of the
region, and that is a fault, not a wrap.*

MMIO is a second sorted table. A peripheral's **index is its insertion
order and never moves**; the sorted view lives in a side table. That matters
because `event_id(peripheral, local)` packs the index into the scheduler's
opaque event tag, and an index that shifted when a lower-based peripheral
was registered later would silently re-point every already-scheduled event.

Byte, halfword and word accesses all reach the peripheral with their
`Width` and their lane: `esp-println` writes the USB-Serial-JTAG FIFO as a
32-bit word and the ROM writes UART0's FIFO as a byte, and both happen in
the same boot.

Three policies:

- **Unmapped is visible.** A read of an address nothing claims returns 0 and
  a write is dropped, but each distinct `(pc, address)` is logged once and
  every one is counted (`unmapped_reads`, `unmapped_writes`,
  `unmapped_sites`). Silence about a wrong memory map is what makes it cost
  a day.
- **Strict mode makes it fatal.** `set_strict(true)` turns the same access
  into `MemoryError::InvalidAccess` — the vision's honest-peripheral policy,
  available to a run that must not guess.
- **Watchpoints fire before the access.** esp-rtos's stack guard is a
  trigger on the guard word; a bus that performed the write and trapped
  afterwards would have already destroyed the evidence. NAPOT decodes as the
  RISC-V debug spec says (`mask = tdata2 ^ (tdata2 + 1)`).

**How many watchpoint slots a machine has is a chip fact, so the bus does not
hold it.** `MAX_WATCHPOINT_SLOTS` is the size of the array the slots live in —
a capacity, not a claim about any chip — and each machine declares its own
count with `SocBus::set_watchpoint_slots`. The ESP32-C6 has four (the RISC-V
debug spec's `mcontrol` triggers) and Xtensa LX6/LX7 have two (`DBREAK`), and
both numbers live in their own chip crates. A fresh `SocBus` has the maximum,
so a machine that declares nothing behaves as it always did; narrowing the
count clears the slots it takes away, because a slot left armed above the
count would be armed and unreachable.

MMIO writes set the **sideband** flag; RAM writes and MMIO reads do not. The
privileged stepper consumes it after store- and system-class instructions to
know whether the interrupt state may have moved.

#### The bus answers twice: `add_ram_alias` (Xtensa plan M6 P02, D2/DD64)

Some parts map one physical SRAM at two addresses — an I-bus view and a
D-bus view of the same bytes. `add_region` refuses to model that as two
regions, and rightly: two regions are two arena spans, and a write through
one would be invisible through the other, silently. `add_peripheral_alias`
already solves the same problem for MMIO (DD38: one state, two decodes).
`SocBus::add_ram_alias(base, len, target_base)` is that rule for RAM.

**One arena, one region, two decodes.** An alias is an address translation,
applied by one private `canonical()` helper **at the top of every entry
point** — `read`, `write`, `fetch_instruction`, `fetch_bytes` and
`load_image` — *before* the region lookup, the watchpoint check, the
memory-cost model, the `bench` counters, the strict-bus code-word checker and
the guest-code span record see the address. So there is exactly one
`RamRegion`, exactly one arena span, and everything downstream sees the
**canonical (target) address**:

- the permission table marks the alias window `PERM_NONE` (it is a gap in the
  arena), so a translated core's inline access there goes out through the
  bus and is translated like any other;
- `take_guest_code_writes` and `take_code_writes` report canonical spans, so
  a translator seeds from the arena's own address and never the door;
- a watchpoint is armed at a canonical address and fires on an access through
  either door; one armed at an alias address never fires;
- a fault raised through the alias carries the canonical address;
- the trace names the door once per `(pc, kind)`:
  `cyc=41 pc=0x42001000 ALIAS store 0x40378010 -> 0x3fc88010`.

The fetch path is the point. The ESP32-S3's shader JIT stores through the
D-bus view of SRAM1 and fetches at `write + 0x6F_0000` — every shader, on the
shipped image (`lp-shader/lpvm-native/src/exec_addr.rs`) — so a translation
that covered loads and stores but not fetch would boot the firmware and fault
on the first shader with no diagnostic. `tests/ram_alias.rs` holds one test
per path, each of which was run against a bus with that path's translation
removed.

**Why it is not the default, and why the classic's aliases stay unmapped.**
DD36's rule is that an alias region is built only when a strict stop — or a
static reading of the image — names a user. M0 measured the classic's SRAM1
I-bus mirror and its RTC-fast I-bus view **unused** by the shipped
`fw-esp32v3` image and by the mask ROM, so the classic names them in its
`memmap` and maps neither (DD3, DD24 R1, DD36): an alias with no user is a
model nobody checks. The C6 has no dual-mapped memory. The S3 is the first
caller, and this crate still holds none of its numbers.

**Cost.** On a bus with no alias, every access pays one `bool` test
(`has_ram_alias`) and nothing else; the table walk and the trace note are out
of line. The same-window A/B of `just bench-emu-c6` is in the P02 PR body.

**Registration is checked, in `add_region`'s voice.** `add_ram_alias` panics
when the target span is not inside one registered region, when the alias
span overlaps a region, another alias, its own target, a peripheral or an
MMIO window, and when the length is zero or a span wraps. `add_region`,
`add_peripheral` and `add_peripheral_alias` refuse the overlap from the
other direction. A RAM alias therefore never reaches MMIO, which is what
lets a translated module's published register window stay a plain compare.

**Snapshot.** `BusScalars` carries the alias table and the trace's
once-per-site set; `restore_scalars` refuses a snapshot whose table is not
this bus's, as `restore_regions` refuses a different region count.

### `periph` — what a peripheral may see

A `Peripheral` gets an offset, a `Width` and a `BusCx`: cycles, the issuing
PC, the hart index, the scheduler, the interrupt source lines, the trace and
the host streams. It never sees the hart's registers and never sees another
peripheral. It raises **source levels** (`irq.set_level(source, bool)`) and
schedules events; turning levels into a CPU interrupt number for a hart is
the chip's matrix, one layer up. `IrqLines` is chip-wide and per-source,
`BusCx.hart` says who is asking — which is the whole of plan PD6's
multi-hart shape until a second hart exists.

**The matrix answers two questions, and which one a machine asks is an ISA
fact.** `CpuIntMatrix::asserted(hart, irq) -> u32` says *which CPU interrupts
are asserted* — the chip's routing applied to the source levels and nothing
else. `CpuIntMatrix::cpu_interrupt(hart, irq) -> Option<u8>` says *which one
the hart should take*. A RISC-V hart asks the second, because its enable mask
and priorities live in the matrix's own MMIO registers, so the bus can resolve
it: `SocBus` answers `Bus::pending_cpu_interrupt` from there. An Xtensa hart
cannot be answered that way at all, because its enable mask is `INTENABLE` and
its level gate is `PS.INTLEVEL`, both **CPU** registers the bus cannot see — so
it asks the first, through `SocBus::pending_cpu_interrupt_mask()`, and resolves
the mask itself. That is the whole reason the trait has two methods instead of
one, and it is why `asserted` is the **required** one: a chip that implemented
only the RISC-V half would leave an Xtensa hart silently taking nothing.

Both are on the every-MMIO-store path, so both carry the same purity contract:
cheap, and a pure function of the levels and the matrix's own configuration —
no scheduling, no logging per call. The mask is a `u32` because a CPU-interrupt
space is 32 wide on both ISAs this crate serves; it is a property of the
interrupt input, not a count of any chip's sources.

### `pins` — the signal fabric (M5 P2, input side M2 P1, input routing M2 P3)

Where a peripheral's output actually goes. A peripheral never sees another
peripheral, so the RMT block cannot read `GPIO.func_out_sel_cfg[18]` to
learn which pad carries its waveform, and the GPIO block cannot ask the RMT
what level its signal is at. The routing is one fact two blocks share —
the shape the interrupt matrix already has (plan DD22): **one state on the
bus, register views writing into it.** `BusCx.pins` is that state for pads
and signals (plan DD34 e):

- a chip's GPIO block is a routing **view**: `route(pad, source, oe, at)`
  and `set_gpio_out(pad, level, at)`;
- an output peripheral calls `drive(signal, level, at)` and never learns
  whether anyone is listening;
- the machine drains `take_edges()` every slice and hands the edges to
  whatever watches the wire — a strip decoder, a raw pin log, the chip's own
  GPIO view;
- something **outside** the chip holds a level on a pad with
  `drive_pad(pad, level, at)` / `release_pad(pad, at)`, and `wire(a, b, at)`
  ties two pads so that whatever one carries the other carries;
- and the other direction, the mirror of the first: a chip's GPIO block
  points a peripheral **input** signal at a pad with
  `route_in(signal, pad, invert)`, and an input peripheral reads
  `input_level(signal)` — `None` when nothing is routed to it, which is a
  different answer from "reads low" and worth a note in the block that
  asked. The C6's RMT receiver is its first caller.

**No chip numbers here either.** The fabric does not know that a C6 calls
signal 71 `RMT_SIG_0` or that 128 means "follow the GPIO output register";
the chip crate decides what a write means and the fabric only remembers and
propagates. An edge is recorded for a pad that has a route, only when its
level actually changes, so an unrouted signal is invisible — as it is on
the pin header.

`pad_level(pad)` is the **resolved** level of the pad — what a scope on the
pin header would read — and the rule is one paragraph, stated in full in the
module doc: collect the pad and everything wired to it; an outside driver
wins over the group's own output, lowest pad number breaking a tie; a
**driving** pad — routed *and* with its GPIO output-enable bit set — wins
over nothing; a group with neither is unobserved and reads low. What makes
a pad a driver is `set_gpio_enable` and not the routing alone (M2 P3, plan
DD38): a routed pad whose enable is clear is an input pad, it carries what
the group carries and contributes nothing, and enabling an already-routed
output is itself an edge.
When the side the driver beat was a driving pad and the levels
disagree, the **conflict** is logged with both levels and the cycle and
counted in `conflicts()` — and then ignored. **Never gated**: an emulator
that refused to run because a bench shorted an output would tell you less
than one that says so and carries on. An edge an outside driver causes goes
into the same `take_edges()` stream as one a peripheral caused.

`set_pad_input_enable(pad, on)` records a pad's input enable (a chip's
IO_MUX `fun_ie`). **Recorded, never gated on here**: it does not change what
`pad_level` answers. The chip's GPIO view reads it back to decide whether to
serve the pad's bit in its input register, which is where a chip fact
belongs.

Modelled: the routing, a signal's driven level, a pad's resolved level, the
cycle it changed, an outside driver, a pad-to-pad wire, and the input enable
as a recorded bit. **Not** modelled: output enable (`oen_sel` and the
`enable` bitmap are recorded and reported in the chip's trace note, never
gated on), drive strength, the *value* of a pull (an undriven pad reads low,
not "pulled high"), open-drain, pad filters, analog, and the timing of an
outside edge beyond the cycle the caller stated — no synchroniser, no
metastability, no propagation delay. A logic analyser on the pin would not
show those either.

There is no `unwire`. A wire is a bench fact declared before the run, so a
run is replayable from its flags alone; the method exists and returns an
error saying so, rather than being a silent no-op for a caller that reached
for it.

### `strip` — what was on the wire (M5 P2)

`strip::ws281x::Ws281xDecoder` reads a pad's edges the way a logic analyser
would: a rising edge opens a bit, the falling edge gives its high time, the
datasheet's **±150 ns** says whether that was a zero, a one, or a
`BitError`, and a low of ≥ 50 µs is the latch that closes the frame. It
knows nothing about who produced the edges, which is the point — it is a
**second opinion** on the frames the peripheral's own word log describes,
so the two agreeing is evidence rather than a tautology.

`ChannelTiming` and `ColorOrder` come from `lp-ws281x`, the crate the
firmware encodes with (MIT, on the fence allowlist), so the emulator's idea
of a bit cannot drift from the driver's. `cpu_hz` is a parameter, not a
constant: the C6 runs at 160 MHz and the classic ESP32 at 240. A `Frame`
keeps the **wire** bytes and `unpermute` gives the RGB the driver was
handed, both in the record, so a wrong colour-order assumption shows up as
a difference between two fields instead of silently inside one.

The tolerance is not a knob. A pulse outside ±150 ns is a finding about the
transmitter; widening the window to make something pass would throw away
the only thing that makes this an oracle.

### `regfile` — accept-and-remember

Most of an SoC's register space is a place the firmware writes and later
reads back. `RegFile` is that, plus a short table of the exceptions:
read overrides ("this bit always reads as …"), write-one-pulse ("reads back
0 after a 1-write"), write-one-to-clear, read-only masks, reset values.
Every spin site the M3 discovery found is one line of it. Anything a block
does beyond that table gets a real type.

A read override leaves the stored value alone, so the trace still shows what
the firmware wrote and a real model can replace the stub later without
re-deriving state.

### `trace` — the bus log

The instrument the vendor emulator does not have. One line per MMIO access:

```text
cyc=41288 pc=0x42009a1c R4 TIMG0+0x068 rtccalicfg = 0x00000000
```

with a per-block filter, an `UNMAPPED` line class that ignores the filter,
and a **spin detector**: the same `(pc, address)` read N times (default
10,000) with no intervening write emits one line —

```text
cyc=999 pc=0x42009a1c SPIN SYSTIMER+0x004 unit0_op = 0x00000000 x1000
```

— which answers "which status bit is the blob waiting on?" in one line
instead of a day.

### `regnames` and the generator

`RegNames` is `offset → name`, binary-searched, with byte-lane offsets
rounded down to their register. Tables are **generated**, never written by
hand:

```bash
scripts/emu/pac-regnames.py           # regenerate
just lint-emu-regnames                # check (wired into `just check-lint`)
```

The generator reads the `#[doc = "0xNN - …"]` offset comments svd2rust
writes in the `esp32c6` PAC — the same source the M3 register inventory came
from — and every output file carries a provenance header naming the repo,
the crate path and version, the svd2rust version read out of the crate, and
the vendored licence (`licenses/esp-pacs-MIT.txt`), per
`docs/adr/2026-07-29-license-provenance-discipline.md`.

Names are block-local (`fifo`, `unit0load.hi`), not `uart0.fifo`: the block
token in a trace line comes from the peripheral *instance* (`UART0` vs
`UART1`), while a PAC register block is a *type* several instances share.
`RegNames::block` carries the type name for anyone who wants it.

The two tables under `tests/regs/` are the generator's proof — a flat block
and one with arrays and clusters — not this crate's data. From P4 on the
generator writes the real tables into the chip crate.

### `host` — where the bytes go

`HostSinks` is a set of named bidirectional byte streams; a peripheral holds
a `StreamId` and cannot tell whether its bytes end in a `Vec`, on stdout, or
on a socket. The RX side is a **scripted** source by default
(`at_cycle → bytes`), because host connect timing was the one drift the
vendor emulator showed and plan PD5 says wall clock never enters the
machine.

### `elf` — the PT_LOAD view

`lp-riscv-elf` presents an emulator-guest memory image, not program headers.
`ElfImage` keeps `vaddr` and `paddr` separate (on ESP images they differ, and
using `vaddr` for both is how `.rtc_fast` ends up in the wrong place),
carries per-segment flags and the zero-fill tail, and looks symbols up by
name and by address — what the ROM intercept table and `--probe` need.

The view accepts two `e_machine` values, `EM_RISCV` and `EM_XTENSA`
(`ACCEPTED_MACHINES`), and rejects everything else as
`ElfError::UnsupportedMachine`. That is not a hole in the neutrality rule: an
ELF machine number is a **format** constant from the ELF specification's `EM_*`
table, not a chip number — `EM_XTENSA` says LX6/LX7 no more than `EM_RISCV`
says C6 — and nothing else about the parse is machine-dependent. A `PT_LOAD`
is a `PT_LOAD`.

## Tests

```bash
cargo test -p lp-emu-core -p lp-emu-esp-common
```

`tests/elf_image.rs` reads a real rv32 firmware ELF **if one is already on
disk** and prints a notice and passes if not; building firmware from a host
crate's tests would make them depend on the rv32 toolchain. Point it
anywhere with `LP_EMU_TEST_ELF=<path>`.
