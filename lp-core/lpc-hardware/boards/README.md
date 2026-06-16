# Hardware Board Manifests

This directory contains checked-in hardware manifests for boards LightPlayer can
run on. A manifest describes the board metadata, known board-visible labels, and
claimable resources such as GPIOs, RMT timing channels, and radios.

The default layout is:

```text
boards/
  vendor/
    product.toml
```

The manifest id must match that path, for example:

```toml
id = "seeed/xiao-esp32-c6"
```

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
directory and records `[[board_label]]` entries plus matching `[[gpio]]`
resources.

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

```toml
id = "vendor/product"
target = "esp32c6"
vendor = "Vendor"
product = "Product"
description = "Board profile."
url = "https://example.com/board"
```

Board-visible labels are optional mapping notes for humans and calibration:

```toml
[[board_label]]
label = "D10"
gpio = "/gpio/18"
status = "assigned"
```

GPIO resources are claimable hardware resources:

```toml
[[gpio]]
address = "/gpio/18"
display_label = "D10"
capabilities = [
    "gpio-output",
    "gpio-input",
]
aliases = [
    "IO18",
    "GPIO18",
]
```

Non-GPIO resources use `[[resource]]`:

```toml
[[resource]]
address = "/rmt/ws281x0"
display_label = "RMT WS281x 0"
capabilities = [
    "rmt",
    "ws281x-output",
]
```

Use `reserved_reason` for known-dangerous or unavailable resources:

```toml
reserved_reason = "crashed during manual GPIO scan; keep skipped until recalibrated"
```

## Validation

Before committing a manifest change, run:

```bash
cargo run -p lp-cli -- hardware manifest validate
cargo test -p lpc-hardware
```

`hardware manifest validate` checks TOML shape, duplicate addresses, required
metadata, URL format, and manifest ids. `cargo test -p lpc-hardware` also
exercises the checked-in default ESP32-C6 manifest.
