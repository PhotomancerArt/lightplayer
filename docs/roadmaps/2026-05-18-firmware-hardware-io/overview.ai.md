# Firmware Hardware IO Roadmap

## Motivation And Rationale

LightPlayer needs a modest hardware concept before the firmware can support real user-facing IO.
The immediate pressure is LED output pin selection, GPIO buttons, and the wireless button/sign use
case. The longer-term pressure is that pins, peripherals, radio, sensors, and virtual emu devices all
need the same basic vocabulary: what exists, what it can do, and who is using it right now.

This roadmap keeps that vocabulary small. It introduces static board metadata, runtime resource
claims, and driver-facing leases. It does not redesign the rendering engine, the output pipeline, or
the on-device shader compiler.

## Architecture And Design

The target shape is:

```text
Board manifest
  describes GPIO/peripherals, labels, capabilities, and off-limits policy
        |
        v
Hardware registry
  enumerates resources and atomically claims/releases them
        |
        +--> Output provider claims GPIO + RMT, then drives WS281x
        |
        +--> Button input claims GPIO input with pull-up, then emits events
        |
        +--> Radio service claims ESP-NOW/WiFi radio, then sends/drains channel messages
```

The registry should be separate from actual drivers. A driver should not decide whether a pin is
safe, already used, or board-attached. It should receive the HAL resources and a lease proving that
the caller has claimed the relevant hardware.

At the shared boundary, prefer string-capable hardware addresses such as `"/gpio/18"`, with typed
IDs internally. Numeric pins should remain accepted for existing output nodes, but the normalized
model should not be numeric-only.

Resource claims should support bundles. A WS281x output on ESP32-C6 currently needs both a GPIO pin
and the single usable RMT channel. Two outputs on the same pin should fail because the GPIO is
claimed. Two outputs on different pins should fail cleanly if they both require the same RMT channel.

`fw-emu` should use the same shared model with virtual board metadata and in-memory claims. That
gives host and firmware tests the same conflict behavior even when there is no real GPIO.

## Alternatives Considered

- Put pin tracking inside `OutputProvider` only. This is enough for duplicate LED outputs but does
  not help buttons, radio, scans, board policy, or future non-output devices.
- Put hardware awareness inside the RMT driver. This couples a protocol driver to board policy and
  makes RMT contention harder to report at the project level.
- Keep numeric GPIO IDs everywhere. This is convenient now, but it does not scale to IO expanders,
  RPi GPIO naming, virtual devices, radio resources, or user-visible labels.
- Build a full hardware abstraction now. Rejected because the current needs are concrete and small:
  output pin selection, one button wiring pattern, and ESP-NOW events.

## Risks

- ESP32 HAL GPIO types are concrete and ownership-heavy. Dynamic pin selection may require a small
  typed dispatch table rather than a fully generic runtime pin object.
- Board metadata may be incomplete at first. The manifest should be conservative and explain
  reserved/dangerous pins instead of pretending all pins are equivalent.
- RMT support is currently effectively single-channel. The roadmap should expose this as a resource
  limitation with a clear error, not hide it behind confusing output failures.
- ESP-NOW uses the WiFi/radio stack and optional dependencies. It should stay out of default
  firmware until intentionally enabled or made production-safe. This roadmap should define the
  hardware channel message source/sink, not LightPlayer event semantics.
- Button input needs debouncing and event semantics. Avoid overfitting it to the first big red
  button, but keep the first API direct.

## Scope Estimate

This is four modest milestones. The first two are implementation-shaped and can stay small. The
radio milestone should promote existing smoke-test code carefully, but should still avoid building a
general wireless bus. The final milestone ties the pieces together for the fyeah sign slice and runs
the broad validation commands.
