# Phase 1: Writer And Trait Foundation

## Scope Of Phase

Define the real `SlotCodec` contract and rename/abstract the writer side enough that generated code can target format-neutral writer cursors.

In scope:

- add `SlotCodec`
- add or rename format-neutral writer types
- re-export the new public codec surface from `lpc-model::slot_codec`
- keep JSON as the only actual writer backend for now
- update existing direct imports in tests/generated code as needed

Out of scope:

- generating record codec impls
- deleting `mockup_codec_policy()`
- implementing every primitive/container codec

## Code Organization Reminders

- Prefer `lpc-model/src/slot_codec/slot_codec.rs` for the trait.
- Prefer `lpc-model/src/slot_codec/slot_writer.rs` for writer cursors and sink trait.
- Keep JSON backend mechanics close to the existing writer implementation.
- Do not introduce an in-memory serialization tree.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Start from `lpc-model/src/slot_codec/slot_json_writer.rs`.

Introduce the public names:

- `SlotWrite`
- `SlotWriteError`
- `SlotWriter`
- `SlotValueWriter`
- `SlotObjectWriter`
- `SlotArrayWriter`

The simplest implementation may be a rename of the existing JSON writer types while the concrete writer still emits JSON. If compatibility aliases reduce churn, use them temporarily:

```rust
pub type SlotJsonWrite = SlotWrite;
pub type SlotJsonWriter<W> = SlotWriter<W>;
pub type SlotJsonValue<'a, W> = SlotValueWriter<'a, W>;
```

Add:

```rust
pub trait SlotCodec: Sized {
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource;

    fn write_slot<W>(
        &self,
        value: SlotValueWriter<'_, W>,
    ) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite;

    fn should_write_slot(&self) -> bool {
        true
    }
}
```

Re-export it from `lpc-model/src/slot_codec/mod.rs` and likely `lpc-model/src/lib.rs`.

Add small tests proving a trivial manual `SlotCodec` impl can read and write one object or value through the existing JSON path.

## Validate

```bash
cargo test -p lpc-model slot_codec
cargo test -p lpc-wire
```
