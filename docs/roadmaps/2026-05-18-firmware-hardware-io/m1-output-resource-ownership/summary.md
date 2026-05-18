# M1 Output Resource Ownership Summary

## What Was Built

- Added `lpc_shared::hardware` with stable hardware addresses, capabilities, resources, manifests,
  bundle claims, leases, a `HardwareRegistry`, and hardware-specific errors.
- Added board-facing metadata fields for display labels, aliases, physical location notes, and
  reserved reasons while keeping internal identity on addresses like `"/gpio/18"`.
- Wired `MemoryOutputProvider` through the hardware registry so WS281x output opens claim GPIO plus
  the virtual single RMT resource atomically.
- Added engine service tests for duplicate GPIO claims and different GPIOs contending for the same
  RMT resource.
- Added a provisional ESP32-C6 board manifest and made `Esp32OutputProvider` claim/release GPIO plus
  RMT resources before opening output.
- Kept ESP32 output transmission on the existing boot-initialized GPIO18 RMT channel and made other
  pins fail clearly without leaking claims.
- Wired `fw-emu` output through the same virtual GPIO plus RMT ownership behavior.
- Parked board manifest calibration notes in M1.1 and future work for dynamic ESP32 GPIO dispatch.

## Decisions For Future Reference

#### Stable Internal Addresses

- **Decision:** Hardware identity is internal address based, such as `"/gpio/18"`, while board
  silkscreen labels are metadata on the selected board profile.
- **Why:** Different ESP32-C6 boards can print different labels for the same HAL GPIO.
- **Rejected alternatives:** Use silkscreen labels as resource identity; keep output pins numeric
  only.
- **Revisit when:** Project files gain a first-class address/string output slot.

#### Registry Owns Claims, Not Drivers

- **Decision:** The registry validates board metadata and active claims; output providers and
  drivers consume leases.
- **Why:** This keeps board policy and resource conflicts out of RMT/WS281x transmission code.
- **Rejected alternatives:** Put duplicate-pin/RMT contention tracking only inside output drivers.
- **Revisit when:** Multiple driver families need shared runtime reconfiguration APIs.

#### ESP32 GPIO18 Remains The Only Active LED Driver Path

- **Decision:** M1 models and claims requested ESP32 GPIO resources, but only opens the existing
  GPIO18-backed RMT channel.
- **Why:** ESP HAL GPIO ownership needs a focused dispatch-table follow-up; M1's goal is ownership
  semantics and clear errors.
- **Rejected alternatives:** Pretend any claimed GPIO can transmit through the GPIO18 channel;
  broaden M1 into full dynamic HAL pin dispatch.
- **Revisit when:** Implementing the future ESP32 dynamic LED pin dispatch item.

#### Calibration Is M1.1

- **Decision:** Interactive board-label calibration is documented separately in M1.1.
- **Why:** It needs a host/firmware workflow with reset handling and human measurement.
- **Rejected alternatives:** Block M1 on measured manifests; bake guessed labels into permanent
  policy.
- **Revisit when:** Static board profiles need to be replaced or corrected from real hardware data.
