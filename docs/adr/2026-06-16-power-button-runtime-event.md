# ADR 2026-06-16: Power Button Runtime Event

## Status

Accepted. Amended 2026-09-22 (switch mode; the request path as landed; see
the amendment at the end).

## Context

LightPlayer needs maker-prototype power behavior on ESP32 dev boards that do not
have a physical power switch. The intended behavior is button-shaped: a short
press remains available to the graph as a normal control trigger, while a long
hold powers the device off by entering ESP32 deep sleep. Wake is tied to the
same physical button and resets the firmware.

This behavior could have been modeled as system-level configuration, but the
project does not yet have a system config surface. The node system already owns
project editing, authored hardware endpoints, graph bindings, and runtime
projection.

## Decision

Power-off-by-button is modeled as a `PowerButton` node.

The node owns the hardware endpoint and timing configuration. Its short press
publishes a transient `click` control map. Its long hold asks the platform to
power off.

The engine does not enter deep sleep directly, and the node never tears the
project down from inside its own tick. The node only *requests*; the request is
carried out at the server level after the frame boundary.

The server owns the power-off transition:

- finish ticking the frame;
- unload all projects, so runtime nodes are destroyed and outputs are closed;
- call the platform to enter power-off.

On ESP32-C6 the platform configures an EXT1 wake on the configured button's
pin, after checking that the board manifest marks that GPIO `deep-sleep-wake`,
then enters deep sleep through the HAL.

## Consequences

The feature stays editable through normal project node artifacts and does not
introduce `system.toml`.

The runtime boundary is explicit: nodes can request server work, but server
lifecycle remains server-owned.

`PowerButton` is special-purpose. It intentionally does not try to model a
general power subsystem yet. If future projects need wake sources that are not
button-attached, that can be introduced as a new node or a system-level config
once the product has a clearer system configuration surface.

## Amendment 2026-09-22: switch mode, the request path, and how this was lost

### History

This ADR and its implementation (`108aa2962`, hardened in `0bc9617c9`) were
written in June and committed to a local `feature/hardware` branch *after* that
branch had merged to main as PR #43. They were never pushed. A later history
rewrite (it removed the story PNGs) re-ided every commit, which made the stale
local branch look 1,234 commits ahead of its remote. A patch-id comparison
showed these two commits were the only content missing from main. The feature
was re-implemented on main in September against the design recorded here, not
cherry-picked: main had moved to JSON artifacts, slot-native defs, per-kind
cargo gates and the `fw-esp32c6` crate in between.

### Switch mode

`PowerButton` gains `mode: "hold" | "switch"`. A `hold` node behaves as above:
a button to ground, pull-up, active low, wake on low. A `switch` node reads a
latching switch that drives the pin high when on, with pull-down and active
high. Off, including off at boot after a settle window, powers off, and the
device wakes on high. The motivating wiring is the PLAYFUL choker: a slide
switch cuts the LED rail, and a series resistor from that switched rail with
the pin's own pull-down tells the chip where the switch is, with no 5 V on
the pin.

The wake level follows from the mode, so the June design's separate `wake`
field (and its single `Ext1Low` value) is gone, as is `action` (only
`DeepSleep` ever existed). Deep sleep is still the only action.

**A switch-mode node does not power off while a USB host is attached.** Deep
sleep drops the USB-Serial-JTAG link, so a choker on a laptop, or connected to
Studio, would otherwise vanish from under the person editing it. "Attached"
means start-of-frame packets are arriving. A computer sends them, a USB power
bank or charger does not, so the choker still sleeps on its battery pack. A
`hold` node ignores host attachment, because a deliberate long press is
explicit.

### The request path, as landed

The June implementation carried a generic `ServerRuntimeEvent` enum on an
event sink threaded through every tick context. It landed smaller:

- The engine defines `PowerService` (`request_power_off`, `host_attached`),
  handed to nodes through `TickContext::power_service()`.
- On a server that service is a `PowerOffQueue`. It checks the wake source with
  the embedder's `PowerPlatform` **when the node asks**, so a pin that cannot
  wake the chip is refused while the device is still running. It then holds the
  request. `LpServer::advance_frame` takes it after ticking, unloads every
  project and calls `PowerPlatform::enter_power_off`.
- Embedders without a platform (host, browser, emulator) install none, and a
  power button there is a node error ("no power service").

The rule is the same one: nodes request, the server acts after the frame. A
second kind of server-level request would earn the generic event channel.

### Outputs go dark on close

Unloading closes outputs, and a WS281x holds its last latched frame for as
long as it has power. In `hold` mode the LED supply usually stays up through
deep sleep. `Esp32OutputProvider::close` now sends one all-black frame and
waits it out before dropping the port. It skips this when a switched power
rail (`docs/adr/2026-08-08-switched-power-rail-mechanism.md`) already has the
channel unpowered.

### Deep sleep and the watchdogs

The C6's RTC watchdog and super watchdog live in the LP domain and keep
counting through deep sleep. The firmware disables both before sleeping.
Otherwise the chip would be reset awake, would boot, find the switch still
off, and sleep again, in a loop.

### Not modelled in the emulator

`lp-emu-esp32c6` has no deep-sleep model. The PMU and LP_AON registers are
accepted and stored, but have no effect. Sleep and wake are checked on the
board. An emulator model is future work, due when a battery board makes sleep
routine.
