# Phase 7: Cleanup, validation, handoff

## Cleanup & validation

1. Grep the working tree for **debug prints**, **stale comments** (“requires `std`” for GLSL), and **temporary TODOs** introduced during implementation; remove or convert to tracked issues.
2. Run **`cargo +nightly fmt`** on all touched crates.
3. Run **`clippy`** on the same scope the repo uses for PRs (e.g. `just clippy` or workspace clippy with cross-only crates excluded — match `.cursorrules` / CI).
4. **Full acceptance:**

```bash
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p lp-server
```

5. Fix **all** warnings that are in scope for this change set.

## Plan cleanup

1. Add **`summary.md`** to this plan directory: bullet list of **what shipped**, **key files**, **follow-ups** (e.g. on-device hardware soak, object/JIT alternatives if ever needed).
2. Move **`docs/plans/2026-03-26-fw-embedded-shader-jit/`** → **`docs/plans-done/`** after implementation is merged or ready to archive.

## Commit

Single conventional commit (or a small logical series) with message like:

```
feat(fw): enable on-device GLSL JIT for fw-emu and fw-esp32

- Split lpir-cranelift glsl vs std; jit() without libstd
- lp-engine real compile_shader for embedded server builds
- Wire fw-emu/fw-esp32; fw-tests + esp32 cargo check acceptance
```

Body: bulleted list of concrete edits.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.
