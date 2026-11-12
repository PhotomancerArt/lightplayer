# Phase 3: Generated Record Codecs

## Scope Of Phase

Teach `lpc-slot-codegen` to generate `SlotCodec` impls for discovered `SlotRecord` structs instead of generating per-field manual read/write expressions from `mockup_codec_policy()`.

In scope:

- reuse `discover_static_slot_records`
- generate `impl SlotCodec for Type`
- generate field constants from discovered slot field names
- use `Default::default()` plus field mutation for record reads
- use `should_write_slot()` for record writes
- keep code compact and helper-driven

Out of scope:

- auto-generating enum codecs
- adopting generated codecs in real domain loaders
- supporting private/non-public slot fields

## Code Organization Reminders

- Keep the discovered record model shared with slot view generation.
- Keep rendering helpers small and named by output concept.
- Generated code should be boring and repetitive; helper traits should carry the complexity.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Add a generated-code path roughly like:

```rust
impl SlotCodec for OutputDriverOptionsConfig {
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource,
    {
        const FIELDS: &[&str] = &["lum_power", "white_point", ...];
        let mut out = Self::default();
        let mut object = value.object()?;
        while let Some(mut prop) = object.next_prop()? {
            match prop.name() {
                "lum_power" => out.lum_power = SlotCodec::read_slot(prop.value())?,
                ...
                other => return Err(prop.unknown_field(other, FIELDS)),
            }
        }
        Ok(out)
    }

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        let mut object = value.object()?;
        if self.lum_power.should_write_slot() {
            self.lum_power.write_slot(object.prop("lum_power")?)?;
        }
        ...
        object.finish()
    }
}
```

Handle discovered field names through existing `#[slot(name = "...")]` support.

If a discovered record does not implement `Default`, either:

- add `Default` to the mockup type if it makes domain sense, or
- make codegen fail clearly for that type for now.

Avoid generated helper functions like `read_dim2u`, `write_glsl_opts`, or per-field constructor expressions. Those are the smell this phase removes.

## Validate

```bash
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup
```
