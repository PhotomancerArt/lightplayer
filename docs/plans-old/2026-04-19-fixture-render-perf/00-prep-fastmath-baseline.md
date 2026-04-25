# Phase 00 — Configure fastmath example + capture baseline profile

**Sub-agent:** main (one-off setup; no point dispatching)
**Parallel:** —
**Profile after:** yes — `p0-baseline`

## Scope of phase

`examples/perf/fastmath/` is currently byte-identical to
`examples/perf/baseline/` and uses the safe-math defaults
(`AddSubMode::Saturating`, `MulMode::Saturating`, `DivMode::Saturating`).
This phase makes `examples/perf/fastmath/` actually use fast-math options
so it becomes the stable fixture for measuring all subsequent perf
deltas.

After the example is updated, capture a baseline profile of
`examples/perf/fastmath` from the **current `HEAD`** (no perf changes
yet). This is the reference all later phases compare against.

**Out of scope:**

- Any code changes to the rendering pipeline.
- Any change to `examples/perf/baseline/` (it stays on safe-math
  defaults — that's its job).
- Touching `examples/basic/` (unstable for profiling, per Q8).

## Code organization reminders

- Single JSON file edit. Nothing to organize.

## Sub-agent reminders

This phase runs on the main agent; the reminders are for the human
reviewing the result.

- Do not generalize to any other example.
- Do not reformat the JSON beyond the edit.
- Do not commit anything else in the same commit.

## Implementation details

Edit `examples/perf/fastmath/src/rainbow.shader/node.json` from:

```json
{
  "glsl_path": "main.glsl",
  "texture_spec": "/src/main.texture",
  "render_order": 0
}
```

to:

```json
{
  "glsl_path": "main.glsl",
  "texture_spec": "/src/main.texture",
  "render_order": 0,
  "glsl_opts": {
    "add_sub": "wrapping",
    "mul": "wrapping",
    "div": "reciprocal"
  }
}
```

Schema reference: `lp-core/lp-model/src/glsl_opts.rs` defines `GlslOpts`
with `add_sub` (`saturating`/`wrapping`), `mul` (`saturating`/`wrapping`),
and `div` (`saturating`/`reciprocal`). All three lowercase via
`#[serde(rename_all = "lowercase")]`. Field is wired into
`ShaderConfig::glsl_opts` in `lp-core/lp-model/src/nodes/shader/config.rs`
behind `#[serde(default)]`, so the field is optional but we set it
explicitly so the example self-documents what it is.

## Validate

Cheap validation (no need for the full `just check` suite — only one JSON
file changed):

```bash
# Confirm the JSON parses as a ShaderConfig.
cargo test -p lp-model --lib -- --quiet
```

If the model crate parses the same struct in tests, that's enough. The
real validation is the profile run itself — if the example loads and
renders, it's well-formed.

## Commit

```bash
git add examples/perf/fastmath/src/rainbow.shader/node.json
git commit -m "$(cat <<'EOF'
chore(examples): enable fast-math glsl_opts on perf/fastmath example

- Set glsl_opts to wrapping/wrapping/reciprocal so the perf/fastmath
  example actually exercises the fast-math path.
- perf/baseline stays on safe-math defaults (saturating/saturating/
  saturating) for a side-by-side reference.

Plan: docs/plans/2026-04-19-fixture-render-perf/00-prep-fastmath-baseline.md
EOF
)"
```

## Capture baseline profile

After the commit, run the profile command and record the resulting dir:

```bash
cargo run -p lp-cli --release -- profile examples/perf/fastmath --note p0-baseline
ls -dt profiles/*--p0-baseline | head -n 1
```

Then read the `report.txt` in that directory and note the top ~10 hot
functions (these become the "before" reference for the rest of the plan).

The profile dir name will look like:
`profiles/2026-04-XXTXX-XX-XX--examples-perf-fastmath--steady-render--p0-baseline/`

## Report back to user

In chat, surface:

- Commit SHA + subject.
- Profile dir path.
- Top 10 entries from `report.txt` (so we have the "before" picture).
