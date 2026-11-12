# Future Work

## CLI Firmware Flash Automation

- **Idea:** Let `lp-cli hardware calibrate` build, flash, reset, and reconnect to calibration
  firmware automatically.
- **Why not now:** The core protocol/session loop should be proven first; flashing adds local
  `espflash`, target, and port-management complexity.
- **Useful context:** Start from `just fwtest-gpio-calibrate-esp32c6` once it exists.

## Comment-Preserving TOML Updates

- **Idea:** Update only the touched manifest fields while preserving comments and formatting.
- **Why not now:** Current manifests are generated/seed files and `HardwareManifestFile::write_toml`
  is enough for first calibration.
- **Useful context:** If hand-edited board manifests become common, add a TOML edit layer around
  `lp-core/lpc-shared/boards/`.

## More Pin Actions

- **Idea:** Extend firmware calibration beyond square wave output to input pull-up/down, ADC, PWM, or
  peripheral capability tests.
- **Why not now:** The immediate problem is mapping board-visible labels to HAL GPIO identities.
- **Useful context:** Keep the firmware protocol action-oriented so future commands can fit beside
  `PULSE <gpio>`.
