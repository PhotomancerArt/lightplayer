# Phase 5: Button Playlist Example And Crossfade Validation

- parallel: -
- sub-agent: main

## Scope Of Phase

Add `examples/button-playlist` and validate the full fyeah-sign local behavior: button down triggers
entry 2, active uses `entry_time`, crossfade works, repeated presses restart active, and the system
returns to idle.

In scope:

- checked-in example files;
- idle and active shaders;
- example loader/render tests;
- crossfade pixel-level or render-level validation where practical.

Out of scope:

- Final art direction for the active shader beyond "fast, bright, clearly active".
- Radio/wireless trigger path.
- Hardware-only diagnostics.

## Code Organization Reminders

- Keep example shader code simple enough to debug on-device.
- Do not copy the full `examples/basic` shader if a smaller idle shader proves the same behavior.
- Use `idle` and `active` naming throughout files and TOML.
- Do not add explicit output bindings to entry child shaders unless a test specifically needs to
  prove explicit binding behavior.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Create:

```text
examples/button-playlist/
  project.toml
  button.toml
  playlist.toml
  idle.toml
  idle.glsl
  active.toml
  active.glsl
  clock.toml
  fixture.toml
  output.toml
```

`project.toml` should include root nodes:

- `output`
- `clock`
- `button`
- `playlist`
- `fixture`

`playlist.toml` should own entry child nodes:

```toml
kind = "Playlist"
idle_entry = 1
default_fade = 0.35

[bindings.time]
source = "bus#time.seconds"

# Optional for clarity. If default visual output binding is complete, this can be omitted.
[bindings.output]
target = "bus#visual.out"

[entries.1]
name = "idle"
fade_after = 0.12
node = { def = { path = "./idle.toml" } }

[entries.2]
name = "active"
duration = 4.0
fade_after = 0.8
node = { def = { path = "./active.toml" } }

[entries.2.bindings.trigger]
source = "bus#trigger"
```

`button.toml` should bind `down`, not `held`:

```toml
kind = "Button"
endpoint = "button:gpio:D9"
id = 1

[bindings.down]
target = "bus#trigger"
```

`active.toml` should bind time from parent playlist:

```toml
kind = "Shader"
source = { path = "active.glsl" }

[bindings.time]
source = "..#entry_time"

[consumed.time]
kind = "value"
value = "f32"
default = 0.0
```

`idle.toml` can use normal/global time. It should omit output binding because the playlist owns it.

`fixture.toml` should consume `bus#visual.out` as usual.

Shader guidance:

- `idle.glsl`: simplified noise/palette visual, calmer than active.
- `active.glsl`: fast moving bright color motion using `time` from playlist `entry_time`.
- Keep GLSL compatible with the existing GLSL frontend and no_std JIT path.

Tests:

- The example loads.
- With no button event, rendered output comes from idle.
- Inject virtual D9 button down; next frames show active visual.
- Active visual restarts on repeated button down instead of toggling to idle.
- After active duration plus fade, output returns to idle.
- A transition frame is a blend, not exactly idle or exactly active, when fade duration is nonzero.

Prefer adding focused engine/project-loader tests over brittle full-image assertions. If a pixel
test is used, choose deterministic shader samples and small render sizes.

## Validate

Run:

```bash
cargo test -p lpc-engine button_playlist
cargo test -p lpc-engine playlist
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```
