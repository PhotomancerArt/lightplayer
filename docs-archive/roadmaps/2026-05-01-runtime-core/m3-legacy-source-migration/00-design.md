# Scope of Work

Milestone 3 migrates the legacy shader / texture / fixture / output authored
source path from per-node `node.json` files to TOML-backed `lpc-source` shapes.

This milestone is an authored-data and loading migration. It does not port the
concrete legacy runtime behavior onto the new core engine, retire
`LegacyProjectRuntime`, or implement real render products. It does switch the
live legacy source sentinel wholesale from `node.json` to `node.toml`, including
builders, templates, examples, and tests.

M3 can be planned while M2.1 is being implemented. M3 should treat M2.1's
runtime-product work as an interface dependency: source loading can prepare the
legacy graph for future runtime products without hard-coding in-flight M2.1
symbol names.

# File Structure

```text
lp-core/
├── lpc-source/src/
│   ├── legacy/
│   │   ├── mod.rs                         # UPDATE: export legacy source loading helpers
│   │   ├── node_config_file.rs            # NEW: node.toml filename, kind/path helpers
│   │   ├── node_loader.rs                 # NEW: generic discovery/read traits + typed TOML load policy
│   │   └── nodes/
│   │       ├── mod.rs                     # UPDATE: exports
│   │       ├── shader/config.rs           # UPDATE: TOML tests/serde shape
│   │       ├── texture/config.rs          # UPDATE: TOML tests/serde shape
│   │       ├── fixture/config.rs          # UPDATE: TOML tests/serde shape
│   │       └── output/config.rs           # UPDATE: TOML tests/serde shape
│   └── node/src_node_config.rs            # VERIFY: remains the core instance config shape
├── lpc-shared/src/
│   └── project/builder.rs                 # UPDATE: write node.toml configs
├── lpc-engine/src/
│   ├── legacy_project/legacy_loader.rs    # UPDATE: use lpc-source node loader, node.toml only
│   ├── legacy_project/project_runtime/core.rs # UPDATE: create/delete/modify sentinel logic
│   ├── legacy/project.rs                  # UPDATE: config reload/init reads node.toml
│   └── nodes/node_runtime.rs              # UPDATE: comments from node.json to node.toml
└── lpc-engine/tests/                      # UPDATE: hot reload/deletion tests use node.toml

lp-base/
└── lpfs/src/
    ├── lib.rs                             # UPDATE: export source trait impl module if needed
    └── lpc_source_legacy.rs               # NEW: implement lpc-source loader traits for LpFs types

lp-cli/src/
└── commands/create/project.rs             # UPDATE: create node.toml projects and tests

lp-app/lpa-server/
├── src/template.rs                        # UPDATE: write current TOML configs
└── tests/                                 # UPDATE: copy node.toml from project builder output

examples/
├── basic/**/node.toml                     # RENAME/CONVERT from node.json
├── basic2/**/node.toml                    # RENAME/CONVERT from node.json
├── fast/**/node.toml                      # RENAME/CONVERT from node.json
└── perf/
    ├── baseline/**/node.toml              # RENAME/CONVERT from node.json
    └── fastmath/**/node.toml              # RENAME/CONVERT from node.json

lp-fw/fw-esp32/src/tests/fluid_demo/
└── ring_geometry.rs                       # UPDATE: example path comments
```

# Conceptual Architecture

```text
project filesystem
  /src/<name>.<legacy-kind>/
    node.toml
    main.glsl or other node-local files

         │
         ▼
lpc-source::legacy
  node_config_file
    owns sentinel filename: node.toml
    owns legacy kind-from-directory-suffix helpers

  node_loader
    owns reusable generic discovery/read traits
    owns typed TOML parse policy:
      .texture -> TextureConfig
      .shader  -> ShaderConfig
      .fixture -> FixtureConfig
      .output  -> OutputConfig

         │ generic trait impls
         ▼
lpfs
  implements source read/discovery traits for LpFs types
  avoids lpc-source -> lpfs dependency cycle

         │
         ▼
lpc-engine::LegacyProjectRuntime
  discovers and loads node.toml configs
  creates legacy runtime nodes
  watches node.toml as the config sentinel
  keeps runtime behavior unchanged

         │
         ▼
examples/builders/templates
  produce node.toml only
  validated by tests and lp-cli profile
```

# Main Components

## Legacy Node Config TOML

The existing legacy config structs remain the compatibility source structs for
the MVP slice:

- `TextureConfig { width, height }`
- `ShaderConfig { glsl_path, texture_spec, render_order, glsl_opts }`
- `FixtureConfig { output_spec, texture_spec, mapping, color_order, transform, brightness, gamma_correction }`
- `OutputConfig::GpioStrip { pin, options }`

M3 adds TOML-focused tests and uses these structs as the typed parse targets for
`node.toml`. The source shape stays explicitly legacy-named; it should not imply
that texture-backed shader -> fixture source is the final core visual model.

## Source-Owned Loading Policy

`lpc-source` should own reusable loading policy because other crates need to
load source, but it should not depend on `lpfs`. `lpfs` already depends on
`lpc-source`, so a direct `lpc-source -> lpfs` dependency would create a crate
cycle.

The pattern should match `ArtifactReadRoot`: define narrow source-owned traits
for reading and discovering legacy node config files, then implement them for
`LpFs` types in `lpfs`.

## Whole-Sale `node.toml` Switch

M3 intentionally switches live code to `node.toml` instead of carrying a
long-term compatibility loader:

- `node.toml` is the only config sentinel in live runtime loading.
- config hot reload watches/modifies `node.toml`;
- node deletion tests delete `node.toml`;
- builders and templates write `node.toml`;
- live examples are converted to `node.toml`.

Historical docs and archived plans can continue to mention `node.json`. Live
comments and tests should use `node.toml`.

Single-file nodes are out of scope. Node directories remain the authored unit.

## Legacy References And Texture Compatibility

Legacy `NodeSpec` references remain supported in the legacy source structs for
M3. The current MVP source path directly expresses shader -> texture, fixture ->
texture, and fixture -> output references. Forcing all of those references
through bus bindings now would mix source migration with runtime graph design.

Texture config remains in the migrated source slice because the current
shader -> fixture path depends on it. It is a compatibility bridge, not the
final render-product abstraction.

## Validation

M3 should validate source parsing, runtime loading, template/project creation,
and used examples:

```bash
cargo test -p lpc-source
cargo test -p lpc-shared
cargo test -p lpc-engine
cargo test -p lpa-server
cargo test -p lp-cli
cargo run -p lp-cli -- profile examples/perf/fastmath --mode steady-render --collect events --note m3-node-toml
```

The `lp-cli profile` command is important because it reads an example project
directory, pushes all files into the emulator-backed server, loads the project,
and drives frames. That proves converted examples work through the live project
load path, not only through serde unit tests.
