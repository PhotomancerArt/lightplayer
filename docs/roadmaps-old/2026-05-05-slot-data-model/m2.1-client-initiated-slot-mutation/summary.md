# M2.1 Summary

## What Changed

This milestone added the first real client-initiated slot mutation slice on top of the M2 pressure harness.

- `lpc-wire` now owns generic slot sync and mutation payloads.
- `lpc-view` now owns a real `SlotMirrorView` with registry/data snapshots, patch application, and pending mutation tracking.
- `lpc-slot-mockup` now depends on the real wire/view crates instead of carrying local mock client and wire payload types.
- `MockRuntime` can dispatch a small set of source/runtime mutations with expected shape/data versions.
- Tests cover accepted mutations, stale data rejection, stale shape rejection, type mismatch, unknown paths, unsupported targets, and the no-optimistic-write client behavior.

## Key Decisions

Client mutation requests are request/ack based for now. The view records pending mutation metadata and waits for the server patch before changing the mirrored value.

Conflicts are explicit. A mutation carries the client's expected root shape version and target data version; the server rejects stale requests instead of guessing how to merge them.

Mutation responses acknowledge request status, while slot patches remain the canonical data transport. An accepted response clears the pending request, but the confirmed value still arrives through normal sync.

The mockup dispatch is intentionally narrow. It proves the protocol with representative source and runtime fields without pretending we have the final mutation router for real nodes.

## Validation

Validated with:

```sh
cargo fmt -p lpc-wire -p lpc-view -p lpc-slot-mockup
cargo test -p lpc-wire
cargo test -p lpc-view
cargo test -p lpc-slot-mockup -- --nocapture --test-threads=1
cargo check -p lpc-wire --features schema-gen
cargo check -p lpc-model --features schema-gen
git diff --check
```
