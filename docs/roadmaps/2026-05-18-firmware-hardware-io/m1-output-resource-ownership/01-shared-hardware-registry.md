# Phase 1: Shared Hardware Registry

## Scope Of Phase

Add `lpc_shared::hardware` with `no_std + alloc` hardware address, manifest, claim, lease, registry,
and error types. Add focused unit tests for address normalization, resource lookup, atomic bundle
claims, duplicate claims, reservation errors, and lease release.

Out of scope: output provider integration, ESP32 HAL pin dispatch, `fw-emu` integration, GPIO input,
and radio resources beyond reserving names/capabilities needed by the shared model.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep `hardware/mod.rs` as declarations and re-exports, not a large implementation sink.
- Put helper functions lower in each file and tests at the bottom.
- Mark temporary code with a clear `TODO` only when the follow-up milestone is specific.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Add `pub mod hardware;` in `lp-core/lpc-shared/src/lib.rs`.

Create:

- `lp-core/lpc-shared/src/hardware/hardware_address.rs`
- `lp-core/lpc-shared/src/hardware/hardware_capability.rs`
- `lp-core/lpc-shared/src/hardware/hardware_resource.rs`
- `lp-core/lpc-shared/src/hardware/hardware_manifest.rs`
- `lp-core/lpc-shared/src/hardware/hardware_claim.rs`
- `lp-core/lpc-shared/src/hardware/hardware_lease.rs`
- `lp-core/lpc-shared/src/hardware/hardware_registry.rs`
- `lp-core/lpc-shared/src/hardware/hardware_error.rs`
- `lp-core/lpc-shared/src/hardware/mod.rs`

Suggested public types:

- `HardwareAddress` wrapping `alloc::string::String`, with constructors:
  `HardwareAddress::new(path: impl Into<String>)`, `HardwareAddress::gpio(u32)`,
  `HardwareAddress::rmt_ws281x(u8)`, and `as_str()`.
- `HardwareCapability` enum with at least `GpioOutput`, `GpioInput`, `Ws281xOutput`, `Rmt`, and
  `Radio`.
- `HardwareResource` with address, capabilities, board-profile display label, aliases, optional
  physical location note, and reserved reason.
- `HardwareManifest` with a board/profile id, human-readable board name, a vector/map of resources,
  helpers to find resources by address, and helpers to construct a virtual single-RMT board for
  tests.
- `HardwareClaim` with a claimant/debug name and a list of addresses.
- `HardwareLease` with a lease id and claimed addresses. Prefer explicit registry release APIs over
  making lease drop mutate shared state if that gets lifetime-heavy.
- `HardwareRegistry` backed by `RefCell` plus `BTreeMap`/`BTreeSet`, suitable for `no_std + alloc`.
- `HardwareError` with variants for invalid address, unknown resource, reserved resource,
  unsupported capability, resource already claimed, duplicate address in claim, and invalid empty
  claim.

Atomicity requirement: `claim_bundle` must validate the whole claim before inserting any active
claim state. Add a test where a bundle includes one free GPIO and one already-claimed RMT resource;
after failure, the GPIO must still be free.

Keep `serde` optionality in mind: `lpc-shared` already depends on `serde`, but the registry does not
need serialization in M1.

Keep labels metadata-only. Claim validation should never use silkscreen labels as identity because
two board profiles can print different labels for the same HAL GPIO, and one board can have aliases
for the same physical header.

## Validate

```bash
cargo test -p lpc-shared hardware
cargo check -p lpc-shared --no-default-features
```
