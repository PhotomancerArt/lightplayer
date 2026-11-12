# Phase 1: Semantic JSON Writer

## Scope Of Phase

Add a small semantic JSON writer in `lpc-wire` that can build JSON objects and arrays without call sites manually writing commas.

In scope:

- Add a minimal `JsonWrite` trait or equivalent no-std-friendly writer abstraction.
- Add `JsonWriter`, `JsonObject`, and `JsonArray` helpers.
- Support object properties and array items with automatic comma handling.
- Support primitive JSON values needed by later phases: null, bool, strings, integer numbers, and raw serde-serialized values.
- Add host tests for punctuation, nesting, string escaping, and chunked write evidence.

Out of scope:

- Project-read response streaming.
- Resource payload special casing.
- ESP transport integration.
- A full general-purpose JSON library.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

Suggested files:

```text
lp-core/lpc-wire/src/json.rs
lp-core/lpc-wire/src/json/json_write.rs
lp-core/lpc-wire/src/json/json_writer.rs
```

Preserve existing imports of `lpc_wire::json::{to_string, from_str, from_slice}`. If converting `json.rs` into a module is noisy, keep `json.rs` as the public facade and include submodules with `#[path = ...]` only if that is the least-bad option. Prefer clean module layout if feasible.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Implement a writer shape roughly like:

```rust
let mut writer = JsonWriter::new(out);
let mut obj = writer.object()?;
obj.prop("name")?.string("shader")?;
obj.prop("revision")?.i64(12)?;
let mut arr = obj.prop("results")?.array()?;
arr.item()?.null()?;
arr.finish()?;
obj.finish()?;
```

Important behavior:

- Commas are automatic for object properties and array items.
- Property names are escaped as JSON strings.
- String values are escaped correctly.
- Finishing an object/array writes the closing delimiter exactly once.
- Dropping without `finish` does not silently write invalid JSON. Prefer explicit `finish` and tests that use it.
- The writer should be `no_std + alloc` compatible at the crate level.

A simple `JsonWrite` trait can look like:

```rust
pub trait JsonWrite {
    type Error;
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
}
```

Add test writers:

- `StringJsonWrite` or `VecJsonWrite` for equivalence tests.
- `ChunkCountingWrite` with tiny chunk threshold to prove multiple writes occur without requiring one big buffer internally.

Use existing `serde_json` in tests to validate generated JSON where useful.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-wire json_writer
cargo test -p lpc-wire
```
