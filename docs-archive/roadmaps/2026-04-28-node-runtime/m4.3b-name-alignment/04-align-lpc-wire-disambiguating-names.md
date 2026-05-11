# Phase 4 — Align `lpc-wire` Disambiguating Names

sub-agent: yes
parallel: -

# Scope of phase

Align `lpc-wire` public names where `Wire*` disambiguates nouns that have
model/source/view/engine siblings:

- `ApiNodeSpecifier` -> `WireNodeSpecifier`
- `SlotIdx` -> `WireSlotIndex`

Keep message/request/response/envelope names natural:

- `Message`
- `ClientMessage`
- `ClientRequest`
- `ServerMessage`
- `FsRequest`
- `FsResponse`

Out of scope:

- Renaming all wire types to `Wire*`.
- Changing serialization formats or wire behavior.
- Moving app/server config types.

# Code organization reminders

- Prefer one concept per file.
- Wire payload files should use file names matching the primary public
  concept when practical.
- Tests belong at the bottom of the module they cover.
- Do not keep compatibility aliases for the old names.

# Sub-agent reminders

- Do not commit.
- Stay within wire naming and call-site updates.
- Do not suppress warnings or weaken tests.
- If a wire rename would alter serialized field names or compatibility,
  stop and report before proceeding.
- Report changed files, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md` and `00-design.md` in this directory first.

Find current definitions:

- `ApiNodeSpecifier` likely lives under `lp-core/lpc-wire/src/project/`.
- `SlotIdx` likely lives near `lp-core/lpc-wire/src/tree/wire_child_kind.rs`
  or the tree module.

Implement:

- Rename `ApiNodeSpecifier` to `WireNodeSpecifier`.
- If file naming is out of pattern, move to
  `project/wire_node_specifier.rs` and update `project/mod.rs`.
- Rename `SlotIdx` to `WireSlotIndex`.
- If file naming is out of pattern, move to
  `tree/wire_slot_index.rs` and update `tree/mod.rs`.
- Update all call sites across `lpc-view`, `lpc-engine`, `lpa-*`, tests,
  and docs that mention active names.

Do not rename:

- `ClientMessage`
- `ClientRequest`
- `ServerMessage`
- `ClientMsgBody`
- `ServerMsgBody`
- `Message`
- `TransportError`

Search targets:

```bash
rg "ApiNodeSpecifier|SlotIdx" .
```

Expected result:

- No active Rust code uses `ApiNodeSpecifier` or `SlotIdx`.
- Active docs use `WireNodeSpecifier` / `WireSlotIndex`.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-wire -p lpc-view -p lpc-engine
cargo test -p lpc-wire
```
