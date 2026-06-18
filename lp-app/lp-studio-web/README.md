# lp-studio-web

`lp-studio-web` is the first static browser shell for LightPlayer Studio.

It renders `lp-studio-core` state and drives the browser-local `browser-worker`
runtime path from `lp-studio-runtime`. It does not own Studio domain behavior and
does not use Dioxus server functions.

## Run

```bash
just studio-dev
```

`studio-dev` builds debug wasm artifacts for `lp-studio-web` and `fw-browser`,
packages them with wasm-bindgen, prepares the worker assets, and serves
`http://127.0.0.1:2820/`.

Use `just studio-web-build` or `just studio-web` when you want the release/static
build path.

## Stories

`lp-studio-web` has a native Dioxus storybook for isolated component states.
Stories live next to the components they exercise, using sibling files such as
`device_panel_stories.rs`.

Run the dev server and open the storybook:

```bash
just studio-dev
```

Then visit `http://127.0.0.1:2820/#/stories`.

Add new stories by:

1. adding a `*_stories.rs` sibling module for the component
2. adding one or more `StoryDescriptor` values
3. adding a `render_story` match arm for each stable story id
4. registering the module in `stories/story_registry.rs`

Use `stories/story_fixtures.rs` for fake but domain-shaped `StudioState`
fixtures. Stories should render real components, not duplicate mock markup.

Generate local PNGs for quick review:

```bash
just studio-story-pngs
```

PNGs are written to `lp-app/lp-studio-web/story-pngs/`, which is gitignored.
They are local review artifacts, not committed visual baselines.

## Boundary

- `lp-studio-core` owns actions, state, effects, diagnostics, and sessions.
- `lp-studio-runtime` owns browser worker protocol flow and demo project loading.
- `lp-studio-web` owns Dioxus components and static presentation.
