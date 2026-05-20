# External Slot Enums Summary

## What was built

- Added `SlotEnumEncoding` to slot shapes, defaulting existing enums to tagged `kind` encoding.
- Added external enum read/write support to the dynamic JSON and TOML slot codecs.
- Added `#[slot(enum_encoding = "external")]` and `#[slot(rename_all = "snake_case")]` to `#[derive(Slotted)]` enum generation.
- Added tests for external scalar, record, and unit payloads, plus invalid external enum shapes.
- Documented slot enum encodings in `docs/design/slots/enum-encoding.md`.

## Decisions for future reference

#### External Encoding Is Opt-In

- **Decision:** Existing enums keep tagged `kind` encoding unless the enum shape explicitly opts into external encoding.
- **Why:** Existing node and project TOML depends on tagged syntax, especially `NodeDef`.
- **Rejected alternatives:** Changing the default enum syntax globally.

#### Field-Key Encoding Deferred

- **Decision:** Field-presence / `#[slot(key)]` discrimination remains future work.
- **Why:** It needs different shape metadata and ambiguity checks, while external encoding was immediately useful and smaller.
- **Rejected alternatives:** Implementing both enum encodings in one change.
- **Revisit when:** Config-like shapes need same-table variant selection with extensible non-key fields.

#### Snake Case Rename Support

- **Decision:** The first rename policy is `rename_all = "snake_case"`, with `#[slot(name = "...")]` still taking precedence.
- **Why:** External tags expose Rust variant names directly to authored TOML; `OptionA -> option_a` is the needed stable API spelling.
- **Rejected alternatives:** Requiring per-variant names for every multi-word variant.
