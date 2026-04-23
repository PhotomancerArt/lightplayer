# Phase 11 — Design-doc updates pointing at `examples/v1/`

> Read [`00-notes.md`](./00-notes.md) and [`00-design.md`](./00-design.md)
> before starting.
>
> **Depends on:** Phase 09 (corpus migration) merged so the new
> canonical paths exist on disk.
>
> Can run in parallel with Phase 10 (round-trip tests) — pure docs.

## Scope of phase

Make the design docs match the M3 reality:

1. **Repath all references** that point at the deleted
   `docs/design/lpfx/{patterns,effects,transitions,stacks,lives,playlists}/`
   TOMLs to the new canonical home at
   `lp-domain/lp-domain/examples/v1/<kind>/`.
2. **Update grammar guidance in `docs/design/lpfx/overview.md`**
   to match what M3 actually shipped — drop the `ui.fader` /
   `ui.stepper` widget hints, drop `unit = "..."` fields, drop
   `type = "f32"` in favor of `kind = "..."`, drop per-entry
   `[entries.transition]` (deferred per Q-D4), and drop
   `[selection]` from Live (deferred per Q-D4).
3. **Add a short "M3 schema baseline" note** to
   `docs/design/lpfx/overview.md` linking the canonical TOML
   grammar to `quantity.md` §10 and the corpus to
   `lp-domain/lp-domain/examples/v1/`.
4. **Sweep sibling docs** (`docs/design/lightplayer/domain.md`,
   `docs/design/lightplayer/quantity.md`, `docs/design/color.md`)
   for stale path references and fix them.

**Out of scope:**

- Rewriting design-doc concepts. The grammar in `quantity.md` §10
  is authoritative; this phase only makes prose match the
  shipped corpus, not vice versa.
- Adding new sections to design docs.
- Touching `docs/roadmaps/` — those are point-in-time milestone
  docs and stay as historical record.
- Touching `docs/plans/` (this is one of them) or
  `docs/plans-old/`.
- Touching `docs/future/` and `docs/future-work/`.
- Recreating the deleted `docs/design/lpfx/concepts.md`
  (already gone; `overview.md` superseded it).

## Conventions

Per [`AGENTS.md`](../../../AGENTS.md):

- Docs are markdown only; no code edits in this phase.
- Cross-doc links use **repo-relative paths** rooted at the doc's
  own directory (e.g. `../../lp-domain/lp-domain/examples/v1/...`
  from a doc under `docs/design/lpfx/`).
- Keep paragraph-level diffs minimal; replace stale URLs and
  examples in place rather than restructuring sections.
- When dropping old TOML field examples, replace them with the
  M3 form; do **not** remove the example block entirely.

## Sub-agent reminders

- Do **not** commit.
- Do **not** edit roadmap, plan, or future-work files.
- Do **not** invent new design concepts; only repath and align
  prose with the shipped corpus.
- Do **not** delete documentation that explains *why* a decision
  was made; only fix stale syntax.
- Verify every new link actually resolves to a file (run
  `git ls-files` filtered to the path, or open in editor).
- Confirm via `rg` that no `docs/design/lpfx/{patterns,effects,
  transitions,stacks,lives,playlists}/` paths remain anywhere
  outside `docs/plans/` / `docs/plans-old/` / `docs/roadmaps/`
  / `docs/future/` / `docs/future-work/`.
- If something blocks, stop and report back.
- Report back: list of changed files, count of stale-link
  replacements, count of grammar-example replacements, before/
  after `rg` counts.

## Concrete edits

### 1. `docs/design/lpfx/overview.md`

Three groups of edits:

#### 1a. UI hint / type vocabulary alignment

The "Conventions (decided so far)" bullets currently say:

```
- **Param types**: any LpsValue / GLSL type — `f32`, `i32`, `bool`,
  `vec2`, `vec3`, `vec4`, structs (eventually).
- **UI hints**: per-kind sub-table on the param.
  - `ui.fader = { step = 0.1 }` for continuous scalars
  - `ui.stepper = { step = 1 }` for discrete scalars
  - `ui.color = {}` for `vec3` / `vec4` color picker
  - `ui.select = { choices = [...] }` for enumerations
  - `ui.checkbox = {}` for `bool`
  - `label`, `unit` are direct fields on the param, independent of `ui`.
```

Replace with M3's vocabulary:

- **Param types** → "Param **kinds** — every param declares
  `kind = "<snake_case>"` from the open `Kind` enum
  (`amplitude`, `ratio`, `phase`, `count`, `color`, `audio_level`,
  ...). The Kind picks the storage type, default
  presentation, default constraint, and default bind."
  Link to `../lightplayer/quantity.md` §3.
- **UI hints** → "Presentation is derived from the param's
  `Kind` (`Kind::default_presentation()`) and may be overridden
  per-param with `presentation = "<variant>"`. The legacy
  `ui.fader` / `ui.stepper` / `ui.color` / `ui.select` /
  `ui.checkbox` sub-tables are gone in v1."
- **`label`, `unit` direct fields** → "`label` stays as a direct
  field on the Slot. `unit` is **gone** in v1: stored values are
  always in the `Kind`'s base unit (radians for Angle, Hz for
  Frequency, etc.) per `quantity.md` §4."

Keep the `time` / `progress` and bus-channel-naming bullets as-is
(they're still right).

#### 1b. `[shader]` section — note the unified form

Add a short "Shader sources" bullet after "Param section":

> - **Shader source** is declared in a single `[shader]` table
>   with exactly one of `glsl = "..."` (inline source),
>   `file = "main.glsl"` (sibling file; language inferred from
>   extension), or `builtin = "fluid"` (built-in Rust impl). The
>   former `[builtin]` block is gone in v1.

#### 1c. Bindings — drop `type = "f32"` from the example

Section "Where bindings are declared" / "1. Inline default on the
declaration" currently shows:

```toml
[params.time]
type = "f32"
bind = { bus = "time" }
```

Replace with the M3 form:

```toml
[params.time]
kind = "phase"
bind = { bus = "time" }
```

(Phase is the Kind for `time`-the-shader-uniform per
`quantity.md` §3. If unsure, mirror whatever
`examples/v1/patterns/rainbow.pattern.toml` actually emits — it's
the canonical answer.)

#### 1d. Transition section — drop per-entry override mention

Current bullet:

> - **Transition section**: always `[transition]`. Live and
>   Playlist both use the same key. Playlist allows
>   `[entries.transition]` for per-entry override.

Replace with:

> - **Transition section**: always `[transition]`. Live and
>   Playlist both declare a single playlist-wide `[transition]`.
>   Per-entry transition overrides are deferred (see M3 plan
>   notes).

#### 1e. New "M3 schema baseline" pointer at the top

Right under the existing intro paragraph, before the
"Conventions" section, add:

> ## Schema baseline
>
> The canonical v1 example corpus lives at
> [`lp-domain/lp-domain/examples/v1/`](../../../lp-domain/lp-domain/examples/v1/),
> exercising all six Visual kinds (Pattern, Effect, Transition,
> Stack, Live, Playlist). The TOML grammar that drives the
> `[params]` section is locked in [`quantity.md` §10](../lightplayer/quantity.md#10-toml-grammar).
> Each example sets `schema_version = 1` as the first field; M5
> introduces the migration framework that will let v2 examples
> live alongside v1 in `examples/v1/<kind>/history/`.

(Adjust the relative path to `examples/v1/` if the workspace
layout requires more `../` hops; verify before committing.)

#### 1f. (Optional) Open questions — touch up

The "Open" section currently includes "Auto-route by type."
Leave it alone — it's still open. Same for the "Priority
computation for Live shows" section — explicitly out of scope
per Q-D4.

### 2. `docs/design/lightplayer/domain.md` (sweep)

Run `rg "design/lpfx/(patterns|effects|transitions|stacks|lives|playlists)/"
docs/design/lightplayer/domain.md` and rewrite each hit to point
at `lp-domain/lp-domain/examples/v1/...` with appropriate
`../../` hops. If the doc currently embeds an old-grammar TOML
snippet, replace the snippet with the corresponding `examples/v1`
file's contents (or with an excerpt — keep the snippet under ~15
lines).

### 3. `docs/design/lightplayer/quantity.md` (sweep)

Same `rg` pattern. The grammar in §10 should already match
what M3 implemented; if it doesn't, **stop and report** —
the design doc is authoritative; the implementation must match
it, not the other way around. Only fix path references, never
the grammar itself.

### 4. `docs/design/color.md` (sweep)

Same `rg` pattern. Likely fewer hits; the doc focuses on the
runtime `Color` value, not on TOML examples.

### 5. Any other `docs/design/**` (sweep)

```bash
rg --files-with-matches \
   'design/lpfx/(patterns|effects|transitions|stacks|lives|playlists)/' \
   docs/design/
```

Patch every file the command turns up. **Do not** patch hits
under `docs/roadmaps/`, `docs/plans/`, `docs/plans-old/`,
`docs/future/`, or `docs/future-work/` — those are historical /
plan-time artifacts.

## Validate

After edits:

```bash
# Stale-path sweep (must turn up zero hits in design/):
rg 'design/lpfx/(patterns|effects|transitions|stacks|lives|playlists)/' docs/design/

# Stale UI-hint vocabulary in lpfx overview (must turn up zero):
rg 'ui\.(fader|stepper|color|select|checkbox)\b' docs/design/lpfx/

# Stale type=... vocabulary in lpfx overview (must turn up zero):
rg '^\s*type\s*=\s*"(f32|i32|bool|vec[234])"' docs/design/lpfx/

# Every new path resolves on disk:
rg -o '(\.\./)*lp-domain/lp-domain/examples/v1/[^)\s"]+' docs/design/ | \
  sort -u | while read -r p; do test -e "${p#*lp-domain/}" \
  || echo "MISSING: $p"; done
```

The last check is a quick spot-check; the agent should also open
2–3 changed docs in the editor and click the new links to
confirm they resolve relative to the doc.

Build the existing markdown linter / link checker if the repo has
one (look in `justfile` for `docs` / `lint` recipes).

## Definition of done

- Every reference under `docs/design/` to the deleted
  `docs/design/lpfx/{patterns,effects,transitions,stacks,lives,playlists}/`
  paths is replaced with the corresponding
  `lp-domain/lp-domain/examples/v1/<kind>/...` path.
- `docs/design/lpfx/overview.md` no longer mentions
  `ui.fader` / `ui.stepper` / `ui.color` / `ui.select` /
  `ui.checkbox` widget sub-tables, no longer uses
  `type = "f32"` style declarations, no longer documents the
  `[entries.transition]` per-entry override.
- `docs/design/lpfx/overview.md` has the new "Schema baseline"
  pointer to `examples/v1/` and `quantity.md` §10.
- `docs/design/lpfx/overview.md` documents the unified
  `[shader]` section with `glsl` / `file` / `builtin` mutex
  variants.
- All `rg` queries above turn up zero hits in `docs/design/`.
- No commit.
- Roadmap, plan, future-work files untouched.

Report back with: list of changed files, exact `rg` before/
after counts for each query above, and any deviations.
