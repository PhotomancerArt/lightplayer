# Phase 2: Mockup Manual Round Trips

## Scope

Add mockup tests with manual typed reader/writer functions that exercise the
new API.

## Out Of Scope

- Derive or macro-generated read/write functions.
- Production model adoption.

## Implementation Details

- Use a representative manual struct with scalars, strings, arrays, objects,
  and length-prefixed base64 bytes.
- Round-trip through JSON syntax events.
- Round-trip through TOML value adapter where TOML supports the values.
- Validate chunked long strings and base64 tuple decode.

## Validation

```bash
cargo test -p lpc-slot-mockup native_stream
```
