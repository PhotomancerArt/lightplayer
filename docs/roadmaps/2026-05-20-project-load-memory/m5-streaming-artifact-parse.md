# M5: Streaming Artifact Parse

## Goal

Reduce peak project-load memory by avoiding full temporary `toml::Value`
representations where typed artifact data can be parsed directly.

## Work

- Measure peak allocation during `NodeArtifact::read_toml` and node definition
  conversion.
- Prototype direct typed deserialization for the highest-volume artifact types.
- Keep error reporting clear enough for project authoring and CLI diagnostics.
- Drop parse buffers as soon as each artifact has been converted to runtime
  form.
- Consider per-artifact arenas only if they simplify temporary cleanup without
  adding resident cost.

## Deliverables

- Direct parsing for the artifact types that dominate load memory.
- Before/after peak memory numbers during project load.
- Tests for valid artifacts, malformed artifacts, and error paths.

## Validation

```bash
cargo run -p lp-cli -- profile examples/button-sign --collect alloc --mode project-load
cargo test -p lpa-server --no-run
cargo test -p fw-tests --test profile_alloc_emu
```

## Implementation Strategy

Small plan for the first artifact family, then repeat if the profile proves the
win. Direct parsing should follow measurement rather than becoming a broad TOML
rewrite by default.
