# Phase 2: Flatten examples/basic Source Layout

## Scope of phase

Migrate `examples/basic` early to the new source layout so the loader work has one canonical target.

In scope:

- Replace `examples/basic/project.json` and `examples/basic/src/*.kind/node.toml` as the canonical source with flat files under `examples/basic/`.
- Create `project.toml`, `texture.toml`, `shader.toml`, `output.toml`, `fixture.toml`, and `shader.glsl`.
- Use `kind = "project"`, `kind = "texture"`, `kind = "shader"`, `kind = "output"`, and `kind = "fixture"`.
- Use `artifact = "./...toml"` in `[nodes.*]`.
- Use `NodeLoc` sibling refs like `texture = "..texture"` and `output = "..output"` in shader/fixture definitions.
- Add focused source parse tests or fixtures that prove the new files parse through the new source types.

Out of scope:

- Do not migrate all other examples yet.
- Do not update server/CLI broad assumptions yet unless a narrow compile break forces it.
- Do not remove old basic files until the plan intentionally does so; moving is okay if tests are updated in this phase.

## Code Organization Reminders

- Follow the repo rule: top to bottom is most important to least important, with tests at the bottom of each Rust file.
- Prefer one concept per file and keep related functionality grouped together.
- Keep helper functions below the public/primary API they support.
- Any temporary code must have a searchable TODO comment and should be removed by the cleanup phase.
- Preserve no_std compatibility in `lpc-model`, `lpc-source`, `lpc-engine`, and shader/runtime paths. Do not add std gates to compile/execute paths.

## Codex / Worker Reminders

- Do not commit. The plan commits at the end as a single unit unless the user explicitly says otherwise.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]` to make the build pass. Fix the issue.
- Do not disable, skip, or weaken existing tests.
- If blocked by ambiguity or an unexpected design issue, stop and report back rather than improvising.
- Report back with: what changed, what was validated, and any deviations from this phase.

## Implementation Details

Target layout:

```text
examples/basic/
├── project.toml
├── texture.toml
├── shader.toml
├── shader.glsl
├── output.toml
└── fixture.toml
```

Example `project.toml` shape:

```toml
kind = "project"
name = "basic"

[nodes.texture]
artifact = "./texture.toml"

[nodes.shader]
artifact = "./shader.toml"

[nodes.output]
artifact = "./output.toml"

[nodes.fixture]
artifact = "./fixture.toml"
```

Keep the same basic behavior: 16x16 texture, rainbow shader source, GPIO strip output, and ring fixture mapping. Adapt field names only as established by Phase 1.

If source tests need a home, prefer `lpc-source` tests that read/parse these files or inline equivalent TOML. Avoid adding broad runtime expectations yet.

## Validate

```bash
cargo test -p lpc-source
```

If a narrow example parse test is added elsewhere, run that package's focused test too.
