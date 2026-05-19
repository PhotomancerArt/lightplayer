# M3.1 Authored Endpoint Specs Summary

## What Changed

- Added `HardwareEndpointSpec` as the shared authored endpoint reference type.
- Added exact endpoint specs to discovered hardware endpoints.
- Switched output authoring from numeric `pin = ...` to `endpoint = "cap:driver:config"`.
- Removed output-node legacy pin compatibility.
- Switched `OutputProvider::open` and engine output flushing to use endpoint specs.
- Updated virtual, emulator, and ESP32 output paths to allocate WS281x through the hardware system
  by endpoint spec.

## Current Specs

- Default WS281x output: `ws281x:rmt:D10`
- Virtual alternate WS281x output in tests: `ws281x:rmt:GPIO19`
- ESP-NOW radio endpoint: `radio:espnow:0`
- Virtual radio endpoint: `radio:virtual:0`
- Virtual GPIO buttons: `button:gpio:<label>`

## Notes

- The spec string is intentionally simple: exactly three colon-separated ASCII parts.
- The third part remains driver-owned config. For now that is usually a terse pin/label token.
- Driver allocation remains responsible for resolving the endpoint into concrete claimed resources.
- The output node no longer accepts `pin = ...`; old configs need to move to `endpoint = "..."`.
