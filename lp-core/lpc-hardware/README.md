# lpc-hardware

`lpc-hardware` owns LightPlayer's board-facing hardware vocabulary: manifests,
claim/lease state, endpoint discovery, and driver traits.

It is `no_std` + `alloc` and is shared by firmware, emulation, and host tests.
The crate should describe what hardware exists, what can be opened, and how
resources are kept from colliding. It should not own rendering policy or engine
behavior.

## Layer Map

```text
Authored project / engine
  uses endpoint specs, writes logical output frames
        |
        v
lpc-shared output providers
  bridge engine-facing APIs to hardware-facing APIs
  own display pipeline options, dithering, interpolation, LUTs
        |
        v
lpc-hardware
  manifests -> resources -> registry claims -> endpoints -> drivers
        |
        v
target driver implementations
  ESP32 RMT, GPIO button, ESP-NOW, virtual test drivers, etc.
        |
        v
physical or emulated hardware
```

The important boundary: `Ws281xOutput` receives already-rendered RGB bytes.
`DisplayPipeline`, `DisplayPipelineOptions`, brightness, interpolation,
dithering, and white-point LUTs live in `lpc-shared`.

## Core Concepts

```text
HwManifest
  |
  +-- HwResource
        |
        +-- HwAddress        /gpio/18, /rmt/ws281x0, /radio/0
        +-- HwCapability     gpio-output, gpio-input, rmt, ws281x-output, radio
        +-- labels/aliases   D10, GPIO18, board location metadata

HwRegistry
  |
  +-- validates resources against the manifest
  +-- accepts HwClaim values from drivers
  +-- returns HardwareLease values for active ownership
  +-- reports HwEndpointStatus for resources

HardwareSystem
  |
  +-- owns registered driver trait objects
  +-- lists endpoints by capability family
  +-- opens endpoints by HwEndpointSpec, HwEndpointId, or HwAddress

HwDriver family traits
  |
  +-- Ws281xDriver -> Ws281xOutput
  +-- ButtonDriver -> ButtonInput
  +-- RadioDriver  -> RadioDevice
```

## Flow

Opening a WS281x output looks like this:

```text
board TOML / default manifest
        |
        v
HwManifest
        |
        v
HwRegistry
        |
        v
HardwareSystem + registered Ws281xDriver
        |
        v
endpoint spec: ws281x:rmt:D10
        |
        v
driver checks endpoint and capabilities
        |
        v
registry claim: /gpio/18 + /rmt/ws281x0
        |
        v
HardwareLease + opened Ws281xOutput
        |
        v
write raw RGB bytes
```

The registry claim is deliberately atomic. If a WS281x output needs both a GPIO
pin and an RMT timing resource, it gets both or neither. That keeps a button,
LED output, radio, or future driver from partially opening hardware and leaving
the board in a confused state.

## Directory Structure

`src/resource/`

Concrete board resources. `HwAddress` is the internal stable address,
`HwCapability` says what a resource can do, and `HwResource` combines those with
human-facing labels and metadata.

`src/manifest/`

Board profiles. `HardwareManifestFile` is the TOML-friendly representation;
`HwManifest` is the runtime form used by the registry.

`src/registry/`

Runtime ownership. Drivers submit `HwClaim`s and receive `HardwareLease`s.
Dropping or closing an opened device releases the lease.

`src/endpoint/`

Openable surfaces reported by drivers. Endpoints connect authored specs such as
`button:gpio:D2` or `ws281x:rmt:D10` to concrete `HwAddress` resources and a
current availability status.

`src/drivers/`

Driver traits and virtual implementations. Firmware crates provide target
drivers where needed; this crate provides common contracts and virtual drivers
for tests/emulation.

`src/hw_system.rs`

The endpoint router. It owns registered drivers, lists endpoints, and opens
devices by spec, ID, or address.

`boards/`

Checked-in board manifest TOML files. Use the `lp-cli hardware manifest` tools
to create, inspect, and validate manifests, and `lp-cli hardware calibrate` to
map board-visible GPIO labels when calibration firmware is running. See
[boards/README.md](boards/README.md) for the workflow.

## Naming

Use the `Hw*` prefix for hardware-domain concepts that would otherwise collide
with model, wire, or engine vocabulary: `HwManifest`, `HwResource`,
`HwRegistry`, `HwEndpoint`, `HwAddress`.

Some older public names still use `Hardware*` where that reads better or avoids
churn in downstream code, such as `HardwareSystem`, `HardwareLease`, and
`HardwareEndpointError`.

## What Does Not Belong Here

Keep these outside `lpc-hardware`:

- Display pipeline options, dithering, interpolation, and color correction.
- Engine/project behavior beyond endpoint specs.
- Target-specific HAL setup that cannot be represented as a common driver
  contract.
- Host-only conveniences that would make the crate stop working in `no_std`.

When in doubt, this crate should answer: "What hardware can this board expose,
who owns it right now, and what raw driver contract opens it?"
