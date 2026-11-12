# M4.4 notes: demo and device hardening

## Why this is separate

M5 should remove old runtime code, not discover late parity bugs. M4.4 gives us
a focused stabilization pass after the M4.1-M4.3 parity milestones have restored
client details, reload behavior, lifecycle cleanup, and runtime behavior parity.

## Validation inventory

- `just demo` against `examples/basic`.
- Focused `lpc-engine` scene render/update tests.
- `lpa-server` load/tick/unload tests.
- ESP32 release check with `esp32c6,server` features.
- Any emulator tests needed to cover real shader compile/execute behavior.

## Things to watch

- Extra frame-buffer copies from `ShaderNode` -> `TextureRenderProduct`.
- Runtime buffer resize behavior in fixture/output hot paths.
- Output close/unregister behavior after reload and stop-all.
- Client cache invalidation when buffers/products are replaced or removed.
- Any build-size or no-std regressions in the embedded JIT path.

## Exit criteria

- The desktop demo is visibly useful again.
- Required host and firmware checks pass.
- Remaining issues are small enough to track in M5 or later feature milestones,
  not parity blockers.
