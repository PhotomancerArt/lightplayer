# Board Manifests

This directory contains checked-in **board manifests** for boards LightPlayer
can run on. A manifest describes the board metadata, known board-visible
labels, and claimable resources such as GPIOs, RMT timing channels, and radios.
(It is not the *firmware* manifest — that is the build's self-description,
embedded in each image; see
`docs/adr/2026-08-01-firmware-manifest-architecture.md`. The CLI subcommand is
still spelled `lp-cli hardware manifest`.)

The default layout is:

```text
boards/
  vendor/
    product.json
```

The manifest id must match that path, for example:

```json
{ "id": "seeed/xiao-esp32-c6" }
```

Two profiles are checked in today: `seeed/xiao-esp32-c6.json` (RISC-V) and
`seeed/xiao-esp32-s3-plus.json` (Xtensa). Read them alongside this document —
they are the authoritative examples of every shape below.

## Tooling

Use `lp-cli hardware manifest` for file management:

```bash
cargo run -p lp-cli -- hardware manifest list
cargo run -p lp-cli -- hardware manifest show seeed/xiao-esp32-c6
cargo run -p lp-cli -- hardware manifest validate
```

Create a new manifest skeleton with:

```bash
cargo run -p lp-cli -- hardware manifest new \
  --target esp32c6 \
  --vendor "Seeed" \
  --product "XIAO ESP32-C6"
```

The tool slugifies the default id from vendor/product. You can override it with
`--id vendor/product`, and use `--description` or `--url` to seed metadata.

`cargo run -p lp-cli -- hardware manifest` with no subcommand opens the
interactive manifest manager when stdin/stdout are terminals.

## Calibration Workflow

Use `lp-cli hardware calibrate` when a board's silkscreen labels need to be
mapped to real GPIO numbers. The calibrator edits the manifest in this
directory and records `board_label` entries plus matching `gpio` resources.

Typical workflow:

1. Create or select a manifest with `hardware manifest`.
2. Flash/run ESP32 calibration firmware built with the `test_gpio_calibrate`
   feature.
3. Run the host calibration UI:

```bash
cargo run -p lp-cli -- hardware calibrate esp32c6 \
  --board seeed/xiao-esp32-c6 \
  --port auto
```

You can jump directly to one board-visible label:

```bash
cargo run -p lp-cli -- hardware calibrate esp32c6 \
  --board seeed/xiao-esp32-c6 \
  --port auto \
  --label D10
```

The calibrator pulses candidate GPIOs over serial. When the connected scope or
LED confirms a match, the tool records the board label and GPIO address. If a
candidate times out or crashes the board, the manifest can keep that GPIO
reserved so normal drivers do not claim it accidentally.

## Manifest Shape

Board metadata lives at the top:

```json
{
  "id": "vendor/product",
  "target": "esp32c6",
  "vendor": "Vendor",
  "product": "Product",
  "description": "Board profile.",
  "url": "https://example.com/board"
}
```

`target` must be a `HardwareTarget` variant (`esp32c6`, `esp32s3`,
`rv32imac_emu`). Adding a new one means adding the variant in
`lp-core/lpc-hardware/src/manifest/hw_target.rs` **and** regenerating
`schemas/hardware.schema.json` — the type feeds that schema and CI checks it.

Board-visible labels are optional mapping notes for humans and calibration:

```json
"board_label": [
  { "label": "D10", "gpio": "/gpio/18", "status": "assigned" },
  { "label": "D4", "status": "not-found" }
]
```

Use `"status": "not-found"` for a silkscreen label the variant does not
actually expose, rather than omitting the entry — the absence is itself a fact
worth recording.

GPIO resources are claimable hardware resources:

```json
"gpio": [
  {
    "address": "/gpio/18",
    "display_label": "D10",
    "capabilities": ["gpio-output", "gpio-input"],
    "aliases": ["IO18", "GPIO18"]
  }
]
```

Non-GPIO resources use `[[resource]]`:

```json
"resource": [
  {
    "address": "/rmt/ws281x0",
    "display_label": "RMT WS281x 0",
    "capabilities": ["rmt", "ws281x-output"]
  }
]
```

Only declare a resource the firmware actually registers a driver for. The S3
profile omits `/radio/0` for exactly this reason: `fw-esp32s3` registers no
radio driver, so the resource could never open.

Use `reserved_reason` for known-dangerous or unavailable resources:

```json
{
  "address": "/gpio/19",
  "reserved_reason": "USB-Serial-JTAG D- — driving it drops the link the board is flashed over"
}
```

## Power gates — "assert this pin or the outputs are dead"

Some boards put the LED supply itself behind a GPIO: the QuinLED dig2go's
GPIO12 cuts strip power entirely, and the Dig-Next-2 has three independently
switched fused outputs. A board declares these with a top-level `power_gate`
list:

```json
"power_gate": [
  {
    "gpio": "/gpio/12",
    "active_level": "high",
    "settle_ms": 25,
    "off_debounce_ms": 5000,
    "note": "who measured these constants, and when"
  }
]
```

This is deliberately **not** a capability and not a claimable resource — a
capability says "this resource can do X"; a gate says the outputs are dead
until it is asserted. The output provider owns the behavior: assert on the
first lit frame, wait `settle_ms` before transmitting (clocking WS281x data
into an unpowered strip phantom-powers the first pixel through its protection
diode), and deassert only after `off_debounce_ms` of all-black frames with no
transmission in flight. See
`docs/adr/2026-08-08-switched-power-rail-mechanism.md`.

Authoring rules:

- **The gate GPIO must also appear in `gpio` with a `reserved_reason`** — a
  driver claiming the pin the rail hangs on is the same physical-damage class
  as a wrong wire. A drift test enforces this for every checked-in board.
- **`feeds` names endpoint addresses** (the `/gpio/N` a wire resolves to),
  never `/rmt/ws281xK` timing slots — on the classic a slot is acquired per
  transmission and is not a stable identity. A board whose gate switches its
  only supply should leave `feeds` empty (= gates every output); an entry
  that matches nothing leaves the rail permanently down, which presents as a
  dead board.
- **Watch for strap pins.** The dig2go's gate is GPIO12 = MTDI, the
  flash-voltage strap: it must be low at boot or the board does not come up.
  Rail-off-at-boot is the provider's contract; the profile's `note` should
  record the strap so nobody "fixes" the idle level.
- `active_level` is a board fact (a user-supplied external relay may invert
  it); `settle_ms`/`off_debounce_ms` are measured records — say who measured
  them in `note`, and mark placeholders as placeholders until a bench
  confirms them.

## Soft limits — measured envelopes, never refusals

`soft_limits` is the block of *measured* records a board×firmware pairing
has actually run clean at — evidence, not policy
(`docs/adr/2026-08-05-manifest-soft-limits-are-measured-records.md`). Every
field is optional; a manifest states only what has been measured, and each
record carries its provenance in `measured` (date, firmware, workload,
observed margins). A record without provenance is a guess and does not
belong here.

```json
"soft_limits": {
  "totalLeds":         { "value": 1500, "measured": "2026-08-05: 5 x 300 at 29.99 fps, 240 s soak ..." },
  "interpolationLeds": { "value": 500,  "measured": "2026-09-06: DERIVED from the per-lamp memory table ..." },
  "ditheringLeds":     { "value": 1000, "measured": "2026-09-06: DERIVED ..." }
}
```

- `totalLeds` — exceeding it **warns and proceeds** at open; Studio draws
  the budget bar from it.
- `interpolationLeds` / `ditheringLeds` — total open LEDs above which the
  output provider opens display pipelines with frame interpolation, then
  temporal dithering, turned **off** (12 and 3 B/LED per port on the
  classic). The tier is a function of the lamps open on the board, never
  of the heap, and the output node's status badge says when it applied.
  Absent = the authored option holds at any scale
  (`docs/adr/2026-09-06-smoothing-degrades-by-measured-lamp-limits.md`).

Only `domraem/dom-z-102.json` carries any of these today. Mark derived
values as derived in `measured`, and replace the provenance — value and
text together — when a bench measurement exists.

## Facts, not reviews

Display copy (`blurb`, notes) states what a board IS — chip, form factor,
notable hardware — never how it ranks or feels. No "best", "easiest",
"clean choice", no claims about testing cadence that the tier system
doesn't already carry. Opinionated review content may come later as its
own clearly-labeled surface; the catalog is a spec sheet. (Ratified at the
firmware-manifest G2 gate, 2026-08-02.)

## Omit what you cannot verify

A wrong GPIO number is a physical-damage class of mistake, not a logic error.
A **missing** manifest entry is a gap someone fills later; a **wrong** one is a
short circuit. So when a board fact is contested or undocumented, leave it out
and say why — in the profile, and ideally in a test.

`seeed/xiao-esp32-s3-plus.json` is the worked example. It deliberately omits:

- the **user LED** — one source says GPIO21 (for the non-Plus board), another
  says GPIO22, which cannot exist on an ESP32-S3 at all;
- the nine **castellated pads** the Plus adds — no source publishes their GPIO
  numbers;
- the **in-package flash/PSRAM pins** (GPIO26-32, plus 33-37 on octal parts) —
  real, but never claimable.

Those omissions are asserted by
`default_esp32s3_manifest_omits_unverified_and_in_package_pins`, so a later
guess fails a test rather than reaching hardware. Prefer that pattern to a
comment.

Vendor docs are not automatically right. Two claims about this board were
refuted against Espressif's primary GPIO reference while writing its profile.
When a vendor page and a primary source disagree, the primary source wins and
the refutation belongs in the profile's `note`.

## The Desktop board — a virtual table, deliberately unlimited

`lightplayer/desktop.json` is the odd one out: it describes a **computer**
running the desktop firmware (`fw-browser` in a Studio tab, `fw-host` on a
machine), and a computer has no pins. Every rule above is about not lying
about silicon; there is no silicon here to lie about, so the profile is a
virtual table sized to say **yes**:

- no `reserved_reason` anywhere — nothing on a computer is dangerous to
  claim, so nothing is withheld;
- one resource per wire label the checked-in catalog authors, grouped in
  bands: `IO0`–`IO47` at `/gpio/0`–`/gpio/47` (label number = address
  number), `D0`–`D13` at `/gpio/100`+, `A01`–`A13` at `/gpio/200`+,
  `B01`–`B13` at `/gpio/220`+. A label is only claimable as an endpoint if
  it is a resource's `display_label` (aliases do not mint endpoints), which
  is why the bands exist at all;
- thirty-two `/rmt/ws281xN` timing resources, so the widest checked-in
  project (`catalog/projects/small-dome`, 26 wires) runs every wire at once;
- `/radio/0`, so `radio:local:0` answers.

"Unlimited" is a property of THIS TABLE, not of a permissive validator: a
Desktop sim resolves its outputs against a manifest exactly like a board sim
does — the manifest simply has room for everything. Its `family` is
`desktop`, which matches no firmware build, so the catalog computes "no
build" for it and always will: the desktop firmware is built, never flashed.

## Display sidecars (`*.display.json`)

Each board may carry a catalog sidecar next to its runtime manifest:

```text
boards/
  vendor/
    product.json          # runtime manifest (this document) — compiled into firmware
    product.display.json  # catalog/drawing metadata — app-side only
```

The sidecar holds everything the boards catalog, provisioning picker, and
diagram renderer need that the runtime manifest must not carry (the runtime
manifest is `include_str!`'d into firmware, where every byte of serde surface
costs flash): display name, support tier, approximate price, purchase URLs,
capability chips, and the drawing block (module outline, USB/buttons/terminals,
per-pin roles and capability cells).

The types, embedded catalog, and JSON schema
(`schemas/board-display.schema.json`) live in `lp-app/lpa-boards`. The drift
tests there (`lpa-boards/tests/manifest_drift.rs`) keep sidecar and runtime
manifest consistent: silkscreen-label→GPIO mappings must agree with
`board_label` entries (including calibration `not-found`), GPIOs the runtime
manifest deliberately omits must not present as claimable pins, and
runtime-reserved GPIOs may not display as plain io. A board may be
display-only (no runtime manifest) only while its SoC has no
`HardwareTarget`; those live on an explicit allowlist in the drift tests with
the reason recorded.

### `default_led_wires` — where the pixels plug in

Each sidecar states which wires LED output goes to by default, **best
first**, using this board's own silkscreen labels:

```json
"default_led_wires": ["IO18", "IO16", "IO14", "IO2"]
```

An entry is a label from the board's own pin/terminal tables, so the
project endpoint is that label with the target prefix —
`ws281x:local:IO18` (`docs/adr/2026-08-03-multi-endpoint-output-node.md`:
the middle segment names the device, the last one names the wire). The
setup flow generates a board's first project onto the **head** of the list;
multi-wire generation is future work, so the tail is documentation for now.

Two gates keep it honest, because a wrong wire is the physical-damage class
of mistake this document is about: `BoardDisplayFile::validate` refuses a
name that is not an output-eligible pin or terminal carrying a gpio, and
the drift tests require every catalog board to declare one *and* fail if
its GPIO is reserved in (or absent from) the runtime manifest.

Order is a board fact where the board has one — the DOM-Z-102 lists its
four fused DATA terminals and omits IO13, which is a spare and is not
level-shifted. On a generic devkit header, one plain `io` pin is the honest
answer; do not pad the list to look thorough.

### Firmware compatibility is computed, not authored

Which firmware a board runs is **derived**, never listed: `family` (the chip,
in espflash spelling — `esp32`, `esp32c6`, `esp32s3`) must equal a build def's
`chip.name`, and `flash_mb` must be at least the build's `flashSizeMb`. See
`lpa-boards/src/firmware_join.rs` and `lp-fw/builds/README.md`.

- `flash_mb` is the join's input and `flash` (`"8 MB"`) is what the reader
  sees; a drift test asserts they agree. **Omit `flash_mb` when the flash size
  is not verified** — an omitted value matches no build, which is the honest
  outcome, and beats guessing a board into a firmware image.
- `firmware_allow` / `firmware_deny` pin exceptions and are empty on every
  board today (a test enforces that). Each entry needs a `reason`. An
  allow-pin relaxes the flash rule only: chip identity is never overridable,
  because a different ISA cannot execute the image at all.

### Support tiers

- **gold** — first-class: tested every release.
- **silver** — supported: tested occasionally.
- **bronze** — community-verified: should work. Boards whose firmware target
  does not exist yet are at most bronze, with a `support_note` saying so.

## Validation

Before committing a manifest change, run:

```bash
cargo run -p lp-cli -- hardware manifest validate
cargo test -p lpc-hardware
```

`hardware manifest validate` checks JSON shape, duplicate addresses, required
metadata, URL format, and manifest ids. `cargo test -p lpc-hardware` also
exercises the checked-in default ESP32-C6 manifest.
