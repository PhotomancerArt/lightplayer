### What was built

- Added custom slot-native `LpValue` read/write support for `ResourceRef`,
  `VisualProduct`, and `ControlProduct`.
- Added `slot_codec::read_dynamic_slot` plus `apply_reader_to_slot`, a generic
  shape-driven reader that mutates default slot objects from syntax streams.
- Added registry-centered read APIs:
  - `SlotShapeRegistry::read_slot_json`
  - `SlotShapeRegistry::read_slot_toml`
  - `SlotShapeRegistry::read_slot_from`
- Added focused model tests for resource/product value codecs and dynamic
  record/map/enum reading.
- Added mockup tests proving registry reads for JSON, TOML, JSON event sources,
  enum payloads, dynamic shader-node shapes, unknown fields, and invalid
  discriminators.

### Decisions for future reference

#### Registry-Centered Reads

- **Decision:** Public dynamic reading starts at `SlotShapeRegistry::read_slot_*`.
- **Why:** The registry owns shape lookup and object factories, so it is the
  natural API boundary.
- **Rejected alternatives:** Free functions as the primary user-facing API.

#### Direct Shape/Data Walk

- **Decision:** `apply_reader_to_slot` walks `SlotShape` and `SlotDataMutAccess`
  directly instead of building `SlotPath` strings.
- **Why:** This avoids needless allocation during streaming reads and keeps the
  reader close to shape metadata.
- **Rejected alternatives:** Implementing dynamic reads by repeatedly calling
  path-based mutation helpers.

#### Missing Fields Stay Default

- **Decision:** Missing fields are not read errors.
- **Why:** Defaults are sentinel model data; validation remains a separate step.
- **Rejected alternatives:** Treating all absent fields as required-field errors.

#### Resource/Product Syntax Is Explicit

- **Decision:** Resource and product `LpValue`s use explicit object forms in the
  slot-native codec.
- **Why:** This is easy to stream, easy to test, and avoids depending on Serde
  for these leaves.
- **Revisit when:** The project settles on a compact string syntax for products
  or resource references.

#### Dynamic Writing Remains Separate

- **Decision:** This milestone only implements dynamic reading.
- **Why:** Reading is the immediate blocker for registry-created objects; dynamic
  writing can reuse different existing writer pieces later.
- **Revisit when:** We replace the remaining mockup/generated write paths.
