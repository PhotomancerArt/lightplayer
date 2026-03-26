# Phase 6: Documentation and acceptance checklist

## Scope of phase

Lock **acceptance criteria**, update **roadmap** / **plan notes**, and give **`fw-esp32`** builders a **single** command reference.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. **`docs/plans/2026-03-26-fw-embedded-shader-jit/00-notes.md`**
   - Add **Acceptance checklist** section with copy-paste commands.

2. **Roadmap** (`docs/roadmaps/2026-03-24-lpir-cranelift/stage-vi-a-embedded-readiness.md` or VI-B/C)
   - Short cross-link: **embedded JIT** validated by **`fw-tests`** + **`fw-esp32 check`**.

3. **Optional:** `README` or `justfile` comment — **how to verify** compiler-in-firmware (if maintainers expect it).

## Tests to write

- None.

## Validate

Manual review of docs. Re-run **full acceptance** once:

```bash
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```
