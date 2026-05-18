# Phase 4: Enums And Codec Surfaces

## Scope Of Phase

Keep the unavoidable mockup-specific boundary explicit without rebuilding the shadow schema table.

In scope:

- implement explicit `SlotCodec` impls for mockup enums/wrappers that need discriminators
- define the small surface policy for public read/write functions
- keep `{ ref = "..." }` / `{ value = ... }` support for binding endpoints if present in the mockup
- keep discriminators friendly and explicit

Out of scope:

- generic enum codegen
- schema versioning
- real project message adoption

## Code Organization Reminders

- Put explicit enum codecs near the generated mockup codec module if they are mockup-only.
- If an enum codec is generally useful to `lpc-model`, put it in the owning model module instead.
- Do not reintroduce per-record field tables.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Surface policy may include:

- public function names such as `read_project_def_json`
- root type names
- root discriminator values like `ProjectDef::KIND`
- wrapper enum variant discriminators like `OutputDef` and `FixtureDef`
- explicit transient/omitted field exceptions until slot metadata owns them

Surface policy must not include:

- every field on every record
- per-field read expressions
- per-field write expressions
- constructor expressions for records

Expected enum read shape:

```rust
let mut object = value.object()?;
let kind = object.expect_discriminator("kind", &["OutputDef", "FixtureDef"])?;
match kind.as_str() {
    "OutputDef" => OutputDef::read_slot_from_object_body(object),
    "FixtureDef" => FixtureDef::read_slot_from_object_body(object),
    _ => unreachable!("validated discriminator"),
}
```

If consuming an already-open object body is awkward, add a small internal helper generated for records. Do not solve it by buffering into a tree.

Expected single-value enum shape:

```toml
source = { ref = "bus#visual.out" }
target = { value = 1.0 }
```

This is explicitly enabled for the enum, not a general untagged inference system.

## Validate

```bash
cargo test -p lpc-slot-mockup generated_shape_codec
cargo test -p lpc-slot-codegen
```

