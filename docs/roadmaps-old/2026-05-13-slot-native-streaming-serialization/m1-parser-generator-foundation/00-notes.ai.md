# M1 Notes: Parser And Generator Foundation

Date: 2026-05-13

## Scope

Implement the first usable foundation for slot-native syntax streams and
manual reader/writer functions.

This milestone should not try to finish generated code or production adoption.
It should create a small real API that later codegen can target.

## Current State

- `lpc-wire::json::JsonWriter` already provides a no-std-friendly direct JSON
  writer with automatic comma handling.
- `lpc-wire::slot::authored_toml` already converts `toml::Value` to and from
  `SlotData`, proving shape-driven storage semantics.
- `lpc-wire::slot::slot_data_json` already writes borrowed slot data directly
  as JSON.
- The missing foundation is a format-neutral syntax event/reader layer plus an
  inverse writer shape that manual and generated typed code can use.

## Target Slice

- Add syntax events for object, prop, array, scalar, chunked string, and null.
- Add a small JSON syntax event parser.
- Add a TOML value adapter that emits the same syntax event model.
- Add a `SlotReader` that can be built from syntax events and provides typed
  helpers for manual construction.
- Add a writer facade over the existing JSON writer.
- Write manual read/write functions in tests for a representative object.
- Round-trip JSON and TOML through the same reader semantics.

## Boundaries

- The first reader may build a small syntax tree internally. Direct streaming
  typed construction remains the design target, but this phase should establish
  event and reader semantics before optimizing away the tree.
- No proc-macro/codegen changes in M1.
- No production loader replacement in M1.
