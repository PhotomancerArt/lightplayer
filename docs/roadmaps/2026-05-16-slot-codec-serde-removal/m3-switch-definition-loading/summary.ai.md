# Summary

## What was built

- `NodeDef` authored TOML reads now consume the wrapper `kind` discriminator and
  hydrate concrete definitions through `SlotShapeRegistry::read_slot_toml`.
- `ProjectLoader` uses the engine's slot shape registry for project and child
  node artifact loading.
- `ProjectBuilder` writes authored project/node TOML payloads from
  `SlotShapeRegistry::write_slot_toml` instead of serializing model structs
  directly with serde.
- Project, output, project-loader, and project-builder tests now exercise the
  SlotCodec TOML path.

## Syntax Notes

- Authored node definition `kind` remains lower-case wrapper metadata such as
  `kind = "project"` and is not part of the concrete slot record.
- Binding endpoints now use the generic SlotCodec enum value shape in authored
  fixture TOML, for example:
  `source = { kind = "Bus", payload = "bus#visual.out" }`.
- `Affine2d` is authored as a 3x3 matrix because its slot value shape is
  `LpType::Mat3x3`, for example:
  `transform = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]`.
- Compact semantic syntax for binding refs remains future work.

## Decisions for future reference

#### Wrapper Kind

- **Decision:** keep `kind` in `NodeDef` wrapper parsing, then remove it before
  SlotCodec record hydration.
- **Why:** concrete definitions stay simple slot records and unknown-field
  checks stay honest.
- **Rejected alternatives:** add `kind` fields to every concrete def; switch
  authored `kind` to shape names.

#### TOML Value Writer

- **Decision:** use SlotCodec to produce `toml::Value`, then use the TOML crate
  only to render that value to text.
- **Why:** model payload serialization is slot-owned while disk formatting stays
  with the TOML library.
- **Rejected alternatives:** keep `toml::to_string(&config)` for model structs;
  hand-write every authored fixture string.
