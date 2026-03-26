# Phase 4: Documentation update

## Scope

Update living documentation to reflect the deleted crates. Leave historical
docs (`plans-done/`, old roadmaps, reports) untouched.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. `README.md` (root)

Remove from the crate table:
- `lp-glsl-cranelift` entry
- `lp-glsl-jit-util` entry
- `lp-glsl-frontend` entry (if listed)
- `lp-glsl-q32-metrics-app` entry (if listed)
- `esp32-glsl-jit` entry (if listed)

Update any prose that mentions "two compiler backends" or similar to reflect
that `lpir-cranelift` is now the single compiler path.

### 2. `lp-glsl/README.md`

Remove directory listing entries for deleted crates. Update the architecture
description to reflect the naga → LPIR → lpir-cranelift chain as the only
compiler path.

### 3. `AGENTS.md`

- Remove the "Do NOT confuse `lpir-cranelift` with `lp-glsl-cranelift`" note
  (the old crate no longer exists).
- Remove `lp-glsl-cranelift` from the "Key Crates" table if present.
- Update any architecture diagrams or prose that references the old crate.

### 4. `.cursor/rules/no-std-compile-path.mdc`

Remove the "Crate confusion warning" section that explains the difference
between `lpir-cranelift` and `lp-glsl-cranelift`. The old crate is gone;
the warning is unnecessary.

### 5. `.cursorrules`

Check for any references to old crates. Update if needed.

### 6. Any active roadmap files

Check `docs/roadmaps/2026-03-24-lpir-cranelift/` for files that still
reference old crates in a forward-looking way (e.g. stage-vii-cleanup.md
itself can stay — it describes what was done). If any other active roadmap
stage references old crates as if they exist, update.

## Validate

No code changes — visual review of updated docs. Optionally:

```bash
rg 'lp-glsl-cranelift|lp-glsl-jit-util|esp32-glsl-jit|lp-glsl-frontend' README.md AGENTS.md .cursorrules .cursor/rules/ lp-glsl/README.md
```

Should return zero matches (or only historical context like "was deleted in
Stage VII").
