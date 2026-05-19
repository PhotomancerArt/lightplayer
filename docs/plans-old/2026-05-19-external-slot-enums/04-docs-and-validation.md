# Phase 4: Docs and Final Validation

## Scope of Phase

Finish documentation, clean up the implementation, and run final validation.

In scope:

- Add project docs for slot enum encodings.
- Ensure new Rust docs are clear and accurate.
- Remove temporary debugging artifacts and stale TODOs.
- Run final targeted checks.
- Fix warnings, formatting issues, and remaining test failures.

Out of scope:

- Migrating shader source definitions.
- Implementing field-presence enum discrimination.
- Broad CI runs beyond the targeted commands below unless explicitly requested.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `docs/design/slots/enum-encoding.md`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot_shape_builder.rs`
- `lp-core/lpc-model/src/slot_codec/dynamic_slot_reader.rs`
- `lp-core/lpc-model/src/slot_codec/dynamic_slot_writer.rs`
- `lp-core/lpc-slot-macros/src/attr.rs`
- `lp-core/lpc-slot-macros/src/slotted_enum.rs`

Documentation should cover:

- Default tagged enum syntax:

  ```toml
  kind = "Variant"
  field = "payload"
  ```

- External enum syntax:

  ```toml
  variant = "payload"
  ```

- Structured external enum syntax:

  ```toml
  [variant]
  x = 10
  y = 10
  ```

- Rust derive usage:

  ```rust
  #[derive(Slotted)]
  #[slot(enum_encoding = "external", rename_all = "snake_case")]
  enum Thing {
      #[default]
      OptionA(ValueSlot<i32>),
  }
  ```

- Variant naming precedence:

  - `#[slot(name = "...")]`
  - `rename_all = "snake_case"`
  - Rust variant name

- Why field-presence / `#[slot(key)]` is not part of this implementation.

Cleanup:

- Run `git diff --check`.
- Search for accidental debugging output and temporary TODOs in changed files.
- Confirm no existing node artifact TOML syntax changes.

## Validate

Run:

```bash
git diff --check
cargo test -p lpc-model slot_codec --lib
cargo test -p lpc-model --features derive --test slotted_enum_derive
cargo test -p lpc-model
cargo test -p lpc-slot-macros
cargo check -p lpc-engine
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

