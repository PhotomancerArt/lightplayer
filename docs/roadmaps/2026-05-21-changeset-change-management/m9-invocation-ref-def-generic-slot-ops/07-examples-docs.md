# Phase 07 — Examples + Docs

**Dispatch:** sub-agent: yes | parallel: - | **Depends on:** 06

## Scope of phase

Update **example projects** and **documentation** to `ref`/`def` TOML wire and
`VariantSet` edit vocabulary.

**In scope:**

- `examples/**/project.toml`, `playlist.toml`, and nested defs with `def = { path`
- `docs/roadmaps/.../change-language.md` — `VariantSet`, Ref|Def TOML, remove SetSlot overload doc
- `docs/design/source-artifacts.md` if invocation syntax documented
- Roadmap `decisions.md` / M9 summary stub
- Re-run `fw-tests` that load examples

**Out of scope:** `docs/plans-old/`, archived roadmaps (optional one-line note only).

## TOML migration pattern

```toml
# before
[nodes.shader]
def = { path = "./shader.toml" }

# after
[nodes.shader]
ref = "./shader.toml"
```

Project inline one-liner:

```toml
# before
[nodes.clock]
def = { kind = "Clock" }

# after
[nodes.clock.def]
kind = "Clock"
```

Playlist:

```toml
# before
node = { def = { path = "./active.toml" } }

# after
node = { ref = "./active.toml" }
# or
[entries.2.node]
ref = "./active.toml"
```

## Example inventory (~15 project.toml + playlist.toml under `examples/`)

Use `rg 'def = \{ path' examples/` for full list.

## Docs

Update `change-language.md` slot op table:

| Op | Use |
|----|-----|
| `VariantSet { path, variant }` | Enum variant (node kind, Ref/Def, nested enums) |
| `SetSlot { path, value }` | Value leaves only |

Remove "SetSlot includes kind, path locators, wiring".

## Validate

```bash
cargo test -p fw-tests --test scene_render_emu
cargo test -p fw-tests --test profile_alloc_emu
rg 'def = \{ path' examples/  # expect zero
```
