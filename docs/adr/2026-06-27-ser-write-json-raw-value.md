# ADR: Patch `ser-write-json` For `serde_json::RawValue`

- **Status:** Accepted
- **Date:** 2026-06-27
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

ESP32 firmware writes server protocol responses with `ser-write-json` so it can
serialize into a bounded writer without allocating one large JSON string.

Some wire DTOs intentionally carry shape-dependent JSON fragments as
`serde_json::value::RawValue`. In particular, `WireSlotData` stores already
encoded slot snapshots such as:

```json
{"kind":"value","changed_at":7,"value":2.0}
```

When `serde_json` serializes `RawValue`, it emits the contained JSON fragment
directly. `ser-write-json` did not know about this `serde_json` convention and
serialized the private marker struct as ordinary JSON instead:

```json
{"$serde_json::private::RawValue":"{\"kind\":\"value\"...}"}
```

Studio then received a slot `data` object without the expected `kind` field and
failed to apply the project read.

## Decision

Patch `ser-write-json` locally under `third_party/ser-write-json` and route the
workspace through `[patch.crates-io]`.

The patch mirrors `serde_json` for the private RawValue token:

- recognize a struct named `$serde_json::private::RawValue`;
- require its single field to use the same token;
- serialize the field string as raw JSON bytes instead of as a quoted string or
  object.

Do not patch `serde`. Do not add a second LightPlayer-specific JSON serializer
alongside `ser-write-json`.

## Consequences

- Firmware keeps the bounded/no-giant-string `ser-write-json` path.
- `WireSlotData` remains a raw JSON wrapper and preserves its existing wire
  semantics.
- `serde_json` and `ser-write-json` now agree on RawValue output for protocol
  messages.
- The repo owns a small local dependency patch that should either be upstreamed
  or revisited if `ser-write-json` is replaced.

## Alternatives Considered

- **Patch `serde`:** rejected. `serde` is generic trait machinery and is not the
  source of this JSON-specific behavior.
- **Parse RawValue into `serde_json::Value` inside `WireSlotData::serialize`:**
  makes the bytes valid but normalizes JSON, changes raw-text equality, and adds
  avoidable firmware allocation/work.
- **Create a LightPlayer raw JSON wrapper only:** still requires serializer
  support for raw JSON emission, so it does not remove the real boundary issue.
- **Replace `ser-write-json` entirely:** plausible later, but too broad for this
  transport/debugging fix and risks adding a second serializer path.

## Follow-ups

- Consider upstreaming the RawValue behavior to `ser-write-json`.
- If serialization keeps being a source of issues, plan a full replacement of
  `ser-write-json` rather than layering another serializer beside it.
