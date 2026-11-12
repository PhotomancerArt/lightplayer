# Phase 4: Cleanup And Docs

## Goal

Leave the model easier to read than we found it.

## Work

- Remove stale `ModelValue` / `ModelType` comments from touched code.
- Finish the `tree` to `root` registry naming cleanup where it is part of the
  public slot shape registry API.
- Keep tests at the bottom of files.
- Update M2.1 notes with any final decisions or follow-up questions.

## Validation

- `cargo test -p lpc-model`
- `cargo test -p lpc-slot-mockup`
- Any focused crate checks required by touched call sites.
