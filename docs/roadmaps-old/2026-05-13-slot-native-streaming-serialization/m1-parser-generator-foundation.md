# Milestone 1: Parser And Generator Foundation

## Title and goal

Build the slot-native reader/writer foundation with manual typed functions and
round-trip tests.

## Suggested plan location

`docs/roadmaps/2026-05-13-slot-native-streaming-serialization/m1-parser-generator-foundation/`

## Scope

In scope:

- Define the syntax event stream used by input adapters.
- Build an initial JSON parser/event source.
- Build a TOML `toml::Value` to event/reader adapter.
- Build the slot-aware reader that wraps syntax input plus `SlotShapeRegistry`.
- Build the output stream concept for JSON/TOML-ish writing.
- Write manual read functions for representative mockup/domain shapes.
- Write manual write functions for the inverse path.
- Add round-trip tests that validate reader and writer semantics.
- Include chunked string handling and a small length-prefixed base64 payload
  test.

Out of scope:

- Proc-macro codegen.
- Broad production loader/message adoption.
- Full schema versioning.
- Removing existing Serde derives.

## Key decisions

- Syntax events stay shape-agnostic.
- Slot/domain semantics live in the reader/writer layer.
- TOML may be tree-backed at first.
- JSON should prove direct streaming early.
- `SlotData` remains a reference path, not the only construction path.

## Deliverables

- Reader/event traits or types in the real codebase.
- JSON input adapter.
- TOML value adapter.
- Output stream/writer abstraction.
- Manual typed reader/writer examples.
- Round-trip tests covering records, maps, options, enums, semantic leaves,
  chunked strings, and base64 resource tuples.
- Notes on rough points discovered before codegen.

## Dependencies

- Existing slot shapes and registry.
- Existing authored TOML codec as reference behavior.
- Mockup model available as a small validation target.

## Execution strategy

Full plan: the API shape is foundational and will affect every later milestone,
so this needs design notes, phase files, and focused validation before coding
spreads into macros or production loaders.
