# Firmware Hardware IO Decisions

#### Hardware Registry Separate From Drivers

- **Decision:** The hardware registry owns metadata, claims, and leases; drivers consume leases and
  HAL resources.
- **Why:** This keeps board policy and conflict reporting out of protocol drivers like RMT/WS281x.
- **Rejected alternatives:** Put pin tracking only in `OutputProvider`; put board knowledge inside
  the RMT driver.
- **Revisit when:** Drivers need runtime reconfiguration that cannot be expressed as lease changes.

#### Claims Are Resource Bundles

- **Decision:** A hardware claim can include multiple resources and must succeed or fail atomically.
- **Why:** WS281x on ESP32 needs both a GPIO and an RMT resource; partial claims would create leaky
  cleanup and bad errors.
- **Rejected alternatives:** Claim GPIO first and discover RMT contention later; rely on driver
  construction failure.

#### String-Capable Hardware Addresses

- **Decision:** Normalize hardware to addresses like `"/gpio/18"` while preserving numeric output
  pins as compatibility input.
- **Why:** Numeric pins do not scale to IO expanders, RPi GPIO, virtual devices, or radio resources.
- **Rejected alternatives:** Keep `u32` as the long-term public model.

#### Start In lpc-shared

- **Decision:** Put initial shared hardware types in `lpc-shared::hardware`.
- **Why:** It is already `no_std + alloc` and sits on the path between model/server/firmware tests
  without adding another crate immediately.
- **Rejected alternatives:** New crate before the surface is proven; firmware-only types.
- **Revisit when:** Hardware APIs grow beyond registry/manifest/claim primitives.

#### Firmware Owns The Hardware Registry

- **Decision:** The firmware/app root creates one device-level hardware registry and passes shared
  access to hardware-facing services such as output, buttons, and future radio.
- **Why:** Hardware is once-per-device, like transports. Projects and nodes should request IO
  behavior through services, not own device resources themselves.
- **Rejected alternatives:** Let each provider own a private registry; attach hardware ownership to
  individual nodes or projects.
- **Revisit when:** A server-level hardware introspection API needs to expose active claims.

#### Device Hardware Manifest Override

- **Decision:** ESP32 firmware uses a compiled default board manifest and falls back to it when
  `/hardware.toml` is absent or invalid.
- **Why:** The checked-in default keeps boot reliable, while a device-local override gives room for
  calibration and board-specific policy without reflashing firmware.
- **Rejected alternatives:** Hardcode the manifest in ESP32 Rust only; fail boot on invalid
  `/hardware.toml`; make manifests project-local.
- **Revisit when:** Runtime hardware editing or hot-reload semantics are designed.

#### ESP-NOW Is The First Radio Path

- **Decision:** Promote ESP-NOW from smoke test to reusable tiny-message transport before exploring
  Thread or richer mesh behavior.
- **Why:** It already validated the same-firmware broadcast/receive path on two ESP32-C6 boards and
  matches the first project.
- **Rejected alternatives:** Start with Thread/OpenThread; design a general wireless bus first.
- **Revisit when:** Reliable delivery, IP networking, or larger synchronized state becomes necessary.

#### Radio API Is Single Consumer First

- **Decision:** The first radio hardware API supports one consumer with channel subscription,
  channel send, and channel drain operations, plus explicit overflow/drop reporting.
- **Why:** The immediate need is basic radio hardware support. Routing, subscriptions, and
  LightPlayer event semantics are separate work.
- **Rejected alternatives:** Design node/event-bus semantics in this roadmap; support multiple
  independent consumers up front.
- **Revisit when:** Multiple runtime features need concurrent access to radio messages.

#### Radio Packets Carry Channel IDs

- **Decision:** Every radio packet carries LightPlayer magic, source device ID, and a `u32` channel
  ID that the radio module understands.
- **Why:** This lets devices ignore unrelated ESP-NOW traffic and lets a single consumer keep
  separate hardware-level message streams without encryption, pairing, or routing.
- **Rejected alternatives:** Single global receive stream only; project-level filtering only.
- **Revisit when:** Channel naming, authorization, or multi-consumer routing becomes necessary.

#### First Button Wiring Is Pull-Up To Ground

- **Decision:** The first GPIO input mode is internal pull-up with a normally-open button to GND.
- **Why:** It matches the near-term big red button hardware and keeps setup easy.
- **Rejected alternatives:** General input mode matrix up front; external resistor assumptions.
- **Revisit when:** Sensor/encoder/input expansion needs appear.
