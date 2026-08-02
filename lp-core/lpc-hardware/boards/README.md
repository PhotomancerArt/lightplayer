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
