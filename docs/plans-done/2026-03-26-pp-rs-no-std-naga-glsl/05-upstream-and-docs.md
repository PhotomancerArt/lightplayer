# Phase 5: Upstream note + documentation

## Scope of phase

Record why the fork exists; **upstream PR is optional** (not scheduled now — revisit later). Link
from **`lp2025`** docs if useful.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. **`light-player/pp-rs` `README.md`:** Problem (naga **`glsl-in`** on **`no_std`**), solution (*
   *`hashbrown` + `alloc`**), compatibility (**crates.io 0.2.1 API**), license.
2. **Optional (later):** Upstream issue/PR — **not** part of closing this plan; revisit if you want
   to drop the **`[patch]`**.
3. **Optional:** Add one line to **`docs/roadmaps/.../stage-vi-a-embedded-readiness.md`** or **VI-C
   ** notes: **“`pp-rs` fork enables `lps-frontend` on RV32.”**

## Validate

- Links in README resolve; no broken internal doc paths.

## Tests to write

- None.
