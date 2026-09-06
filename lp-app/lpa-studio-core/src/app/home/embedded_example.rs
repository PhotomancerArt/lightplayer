//! The compiled-in example packages: the offline/dev content source for
//! the canonical example identities.
//!
//! Examples are first-party published projects (examples vision D1) with
//! bare-slug addresses (`/p/<slug>`, the id tail). Opening one is
//! STATELESS (D2): a transient memory-backed session, nothing installed —
//! an explicit save forks a copy with `SeededFrom { source: id }`
//! provenance (the "Remixed from" line). The home landing and Explore
//! both list this table.
//!
//! Each package's files are `include_bytes!`d from
//! `catalog/<bucket>/<slug>/` (buckets `patterns` and `projects`), so the
//! wasm bundle carries them and the checked-in entry IS what the gallery
//! opens. Ids are bucket-free — `catalog/<slug>` — so reclassifying an
//! entry never changes what a library's "Remixed from" line points at;
//! the pre-catalog spelling `examples/<slug>` is still accepted on lookup
//! (see [`embedded_example`]). Adding an entry means adding its file
//! table here (slug uniqueness is test-pinned — the id tail is the URL).

/// One file in an embedded package: its package-relative path and bytes.
pub type ExampleFile = (&'static str, &'static [u8]);

/// One compiled-in example.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddedExample {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
    /// The package's files, in deploy order (`project.json` first).
    pub files: &'static [ExampleFile],
}

impl EmbeddedExample {
    /// The example's package files as owned (relative path, bytes) pairs.
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        self.files
            .iter()
            .map(|(path, bytes)| ((*path).to_string(), bytes.to_vec()))
            .collect()
    }

    /// The example's canonical bare slug — the id tail
    /// (`catalog/fyeah-sign` → `fyeah-sign`). This is the `/p/<slug>`
    /// address (PD3): example directories are `[a-z0-9-]` names, so the
    /// tail is URL-ready as-is. Uniqueness across the table is pinned by
    /// a test below.
    pub fn slug(&self) -> &'static str {
        self.id.rsplit('/').next().unwrap_or(self.id)
    }
}

/// `catalog/fyeah-sign` — the Studio demo project (see
/// [`crate::app::project::demo_project`] for why this one).
pub static FYEAH_SIGN_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/projects/fyeah-sign/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/projects/fyeah-sign/module.json"),
    ),
    (
        "button.json",
        include_bytes!("../../../../../catalog/projects/fyeah-sign/button.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/projects/fyeah-sign/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../catalog/projects/fyeah-sign/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../catalog/projects/fyeah-sign/output.json"),
    ),
    (
        "playlist.json",
        include_bytes!("../../../../../catalog/projects/fyeah-sign/playlist.json"),
    ),
    (
        "radio.json",
        include_bytes!("../../../../../catalog/projects/fyeah-sign/radio.json"),
    ),
    (
        "idle.json",
        include_bytes!("../../../../../catalog/projects/fyeah-sign/idle.json"),
    ),
    (
        "idle.glsl",
        include_bytes!("../../../../../catalog/projects/fyeah-sign/idle.glsl"),
    ),
    (
        "blast.json",
        include_bytes!("../../../../../catalog/projects/fyeah-sign/blast.json"),
    ),
    (
        "blast.glsl",
        include_bytes!("../../../../../catalog/projects/fyeah-sign/blast.glsl"),
    ),
    (
        "fyeah.map2d.json",
        include_bytes!("../../../../../catalog/projects/fyeah-sign/fyeah.map2d.json"),
    ),
];

/// `catalog/logo-sign` — the brand as a buildable LED piece: a shaped
/// PCB matrix in the outline of the play triangle (map2d `filled_polygon`,
/// count derived from the outline and the pitch) plus "LightPlayer" as a
/// string of single-stroke letter strands, on one canvas that is the
/// landing hero's own stage. Generated from the brand geometry — see
/// `logo_sign_gen.rs` in `lpa-studio-web`, whose in-sync test fails if this
/// package's mapping falls behind the mark.
pub static LOGO_SIGN_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/projects/logo-sign/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/projects/logo-sign/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/projects/logo-sign/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../catalog/projects/logo-sign/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../catalog/projects/logo-sign/output.json"),
    ),
    (
        "shader.json",
        include_bytes!("../../../../../catalog/projects/logo-sign/shader.json"),
    ),
    (
        "shader.glsl",
        include_bytes!("../../../../../catalog/projects/logo-sign/shader.glsl"),
    ),
    (
        "sign.map2d.json",
        include_bytes!("../../../../../catalog/projects/logo-sign/sign.map2d.json"),
    ),
];

/// `catalog/plasma` — one shader, two public knobs. The smallest module
/// whose root panel is not empty: `scale` and the phasor slot's period
/// (bound to the `speed` channel, which carries the whole `PhasorConfig`)
/// are bound to root scope channels, so binding-is-publicity (Q13) puts
/// them on the module card's panel with nothing else authored.
pub static PLASMA_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/patterns/plasma/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/patterns/plasma/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/patterns/plasma/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../catalog/patterns/plasma/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../catalog/patterns/plasma/output.json"),
    ),
    (
        "shader.json",
        include_bytes!("../../../../../catalog/patterns/plasma/shader.json"),
    ),
    (
        "shader.glsl",
        include_bytes!("../../../../../catalog/patterns/plasma/shader.glsl"),
    ),
    (
        "fixture.map2d.json",
        include_bytes!("../../../../../catalog/patterns/plasma/fixture.map2d.json"),
    ),
];

/// `catalog/pulse` — the plainest possible shader: the whole fixture
/// breathes one colour on a phasor, a raised cosine so the floor is never
/// black. The hardware-walk test subject generally: if a strip is dark
/// under `pulse`, that is the wiring or a fault, never the content.
/// Publishes `speed`.
pub static PULSE_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/patterns/pulse/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/patterns/pulse/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/patterns/pulse/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../catalog/patterns/pulse/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../catalog/patterns/pulse/output.json"),
    ),
    (
        "shader.json",
        include_bytes!("../../../../../catalog/patterns/pulse/shader.json"),
    ),
    (
        "shader.glsl",
        include_bytes!("../../../../../catalog/patterns/pulse/shader.glsl"),
    ),
    (
        "fixture.map2d.json",
        include_bytes!("../../../../../catalog/patterns/pulse/fixture.map2d.json"),
    ),
];

/// `catalog/fault-demo` — a shader that compiles fine but FAULTS every
/// frame at run time (fuel exhaustion): the deterministic, non-crashing
/// demo of "a fault is never black" (docs/adr/2026-09-02-fault-is-never-black.md)
/// — the outputs show the red breathe and the device card reads Degraded.
/// Publishes `speed` (the gallery rule needs one root control; the shader
/// itself never reads it).
pub static FAULT_DEMO_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/patterns/fault-demo/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/patterns/fault-demo/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/patterns/fault-demo/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../catalog/patterns/fault-demo/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../catalog/patterns/fault-demo/output.json"),
    ),
    (
        "shader.json",
        include_bytes!("../../../../../catalog/patterns/fault-demo/shader.json"),
    ),
    (
        "shader.glsl",
        include_bytes!("../../../../../catalog/patterns/fault-demo/shader.glsl"),
    ),
    (
        "fixture.map2d.json",
        include_bytes!("../../../../../catalog/patterns/fault-demo/fixture.map2d.json"),
    ),
];

/// `catalog/plasma-duo` — the plasma shader driving TWO fixtures in one
/// module: the disc and a 16×16 grid, each with its own output channel.
/// Exists for the "What's a shader?" docs page ("it gets projected onto
/// your LEDs, regardless of their shape"): one sim, one set of knobs,
/// two shapes reacting together. The shader and clock stay byte-identical
/// with `catalog/plasma` so the docs edit-me story and the gallery
/// example never drift apart.
pub static PLASMA_DUO_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/patterns/plasma-duo/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/patterns/plasma-duo/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/patterns/plasma-duo/clock.json"),
    ),
    (
        "shader.json",
        include_bytes!("../../../../../catalog/patterns/plasma-duo/shader.json"),
    ),
    (
        "shader.glsl",
        include_bytes!("../../../../../catalog/patterns/plasma-duo/shader.glsl"),
    ),
    (
        "disc.json",
        include_bytes!("../../../../../catalog/patterns/plasma-duo/disc.json"),
    ),
    (
        "grid.json",
        include_bytes!("../../../../../catalog/patterns/plasma-duo/grid.json"),
    ),
    (
        "disc_out.json",
        include_bytes!("../../../../../catalog/patterns/plasma-duo/disc_out.json"),
    ),
    (
        "grid_out.json",
        include_bytes!("../../../../../catalog/patterns/plasma-duo/grid_out.json"),
    ),
    (
        "disc.map2d.json",
        include_bytes!("../../../../../catalog/patterns/plasma-duo/disc.map2d.json"),
    ),
    (
        "grid.map2d.json",
        include_bytes!("../../../../../catalog/patterns/plasma-duo/grid.map2d.json"),
    ),
];

/// `catalog/meteor` — a compute/render pair: `sim` integrates meteor heads
/// into a persistent map, `render` draws their tails from it over a
/// node-to-node binding. Publishes `speed`, `count` (a stepped knob) and
/// `decay` on the root panel.
pub static METEOR_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/patterns/meteor/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/patterns/meteor/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/patterns/meteor/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../catalog/patterns/meteor/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../catalog/patterns/meteor/output.json"),
    ),
    (
        "sim.json",
        include_bytes!("../../../../../catalog/patterns/meteor/sim.json"),
    ),
    (
        "sim.glsl",
        include_bytes!("../../../../../catalog/patterns/meteor/sim.glsl"),
    ),
    (
        "render.json",
        include_bytes!("../../../../../catalog/patterns/meteor/render.json"),
    ),
    (
        "render.glsl",
        include_bytes!("../../../../../catalog/patterns/meteor/render.glsl"),
    ),
    (
        "fixture.map2d.json",
        include_bytes!("../../../../../catalog/patterns/meteor/fixture.map2d.json"),
    ),
];

/// `catalog/fire2012` — a WLED port (`mode_fire_2012`) re-authored as a
/// STATELESS 1D shader: upstream's per-cell heat simulation is not ported
/// (the engine cannot express a compute-produced dense scalar array), so
/// the closed form writes down what that simulation settles into. Publishes
/// `speed`, `reach`, `sparks` and `palette`.
pub static FIRE2012_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/patterns/fire2012/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/patterns/fire2012/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/patterns/fire2012/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../catalog/patterns/fire2012/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../catalog/patterns/fire2012/output.json"),
    ),
    (
        "shader.json",
        include_bytes!("../../../../../catalog/patterns/fire2012/shader.json"),
    ),
    (
        "shader.glsl",
        include_bytes!("../../../../../catalog/patterns/fire2012/shader.glsl"),
    ),
    (
        "fixture.map2d.json",
        include_bytes!("../../../../../catalog/patterns/fire2012/fixture.map2d.json"),
    ),
];

/// `catalog/comet` — a WLED port ("Lighthouse", `mode_comet`) authored as
/// a true 1D shader: `vec4 render_1d(float)` and a
/// `OneD { in_2d: Project { extrude-x } }` declaration — the factored
/// default, so a 2D consumer sees the comet swept across the panel.
/// Publishes `speed`, `tail` and `palette`.
pub static COMET_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/patterns/comet/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/patterns/comet/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/patterns/comet/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../catalog/patterns/comet/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../catalog/patterns/comet/output.json"),
    ),
    (
        "shader.json",
        include_bytes!("../../../../../catalog/patterns/comet/shader.json"),
    ),
    (
        "shader.glsl",
        include_bytes!("../../../../../catalog/patterns/comet/shader.glsl"),
    ),
    (
        "fixture.map2d.json",
        include_bytes!("../../../../../catalog/patterns/comet/fixture.map2d.json"),
    ),
];

/// `catalog/palette-waves` — a WLED port (`mode_colorwaves`) and the
/// declared-projection example: `OneD { in_2d: Project { radial } }` on a disc fixture,
/// so the strip the shader is written along arrives as rings. Publishes
/// `speed`, `scale`, `depth` and `palette`.
pub static PALETTE_WAVES_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/patterns/palette-waves/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/patterns/palette-waves/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/patterns/palette-waves/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../catalog/patterns/palette-waves/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../catalog/patterns/palette-waves/output.json"),
    ),
    (
        "shader.json",
        include_bytes!("../../../../../catalog/patterns/palette-waves/shader.json"),
    ),
    (
        "shader.glsl",
        include_bytes!("../../../../../catalog/patterns/palette-waves/shader.glsl"),
    ),
    (
        "fixture.map2d.json",
        include_bytes!("../../../../../catalog/patterns/palette-waves/fixture.map2d.json"),
    ),
];

/// `catalog/zook-dome` — a real 16' geodesic dome: 1500 LEDs as five
/// 300-lamp channels, mapped top-down from the builder's wiring sketch
/// (`scripts/zook-dome/`). The mapping-scale example: rings from the apex
/// cross all five channels with no per-channel configuration.
pub static ZOOK_DOME_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/projects/zook-dome/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/projects/zook-dome/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/projects/zook-dome/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../catalog/projects/zook-dome/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../catalog/projects/zook-dome/output.json"),
    ),
    (
        "shader.json",
        include_bytes!("../../../../../catalog/projects/zook-dome/shader.json"),
    ),
    (
        "shader.glsl",
        include_bytes!("../../../../../catalog/projects/zook-dome/shader.glsl"),
    ),
    (
        "fixture.map2d.json",
        include_bytes!("../../../../../catalog/projects/zook-dome/fixture.map2d.json"),
    ),
];

/// `catalog/small-dome` — Yona's real 16' 2V dome at full scale: 50
/// suspended triangle panels of 119 lamps each (ten 5-way-repeated polygon
/// objects, map2d format 4) AND one always-lit 360-lamp chevron door,
/// scattered across TWO named outputs (the build's two control boxes, 13
/// ports each) with a shared port tail — many-to-many, the patching
/// archetype (`docs/use-cases/2026-08-09-mini-dome.md`), and a
/// desktop-scale stress fixture (6,310 lamps). The `.patch.json` files
/// carry the as-built install as format-2 path-identity rows
/// (`/band-a/3`), reversal and stride-stepped rotation included; all six
/// wiring artifacts regenerate via `cargo run -p lpt-geodome`.
pub static SMALL_DOME_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/projects/small-dome/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/projects/small-dome/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/projects/small-dome/clock.json"),
    ),
    (
        "editor.json",
        include_bytes!("../../../../../catalog/projects/small-dome/editor.json"),
    ),
    (
        "out_a.json",
        include_bytes!("../../../../../catalog/projects/small-dome/out_a.json"),
    ),
    (
        "out_b.json",
        include_bytes!("../../../../../catalog/projects/small-dome/out_b.json"),
    ),
    (
        "dome/module.json",
        include_bytes!("../../../../../catalog/projects/small-dome/dome/module.json"),
    ),
    (
        "dome/dome.json",
        include_bytes!("../../../../../catalog/projects/small-dome/dome/dome.json"),
    ),
    (
        "dome/dome.map2d.json",
        include_bytes!("../../../../../catalog/projects/small-dome/dome/dome.map2d.json"),
    ),
    (
        "dome/dome.patch.json",
        include_bytes!("../../../../../catalog/projects/small-dome/dome/dome.patch.json"),
    ),
    (
        "dome/dome_sky.json",
        include_bytes!("../../../../../catalog/projects/small-dome/dome/dome_sky.json"),
    ),
    (
        "dome/dome_sky.glsl",
        include_bytes!("../../../../../catalog/projects/small-dome/dome/dome_sky.glsl"),
    ),
    (
        "doors/module.json",
        include_bytes!("../../../../../catalog/projects/small-dome/doors/module.json"),
    ),
    (
        "doors/doors.json",
        include_bytes!("../../../../../catalog/projects/small-dome/doors/doors.json"),
    ),
    (
        "doors/doors.map2d.json",
        include_bytes!("../../../../../catalog/projects/small-dome/doors/doors.map2d.json"),
    ),
    (
        "doors/doors.patch.json",
        include_bytes!("../../../../../catalog/projects/small-dome/doors/doors.patch.json"),
    ),
    (
        "doors/door_warm.json",
        include_bytes!("../../../../../catalog/projects/small-dome/doors/door_warm.json"),
    ),
    (
        "doors/door_warm.glsl",
        include_bytes!("../../../../../catalog/projects/small-dome/doors/door_warm.glsl"),
    ),
];

/// `catalog/peach-1d` — the stained-glass peach declared 1D: two fixtures
/// (body and leaves) on ONE wire, each running a `render_1d` shader along
/// the strand, with `strip_order_meaningful` selecting wire order over the
/// map. Its `.patch.json` files are byte-identical to `catalog/peach-2d`'s
/// — the patch is where the lamps land, not what they are told to draw.
pub static PEACH_1D_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/clock.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/output.json"),
    ),
    (
        "body/module.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/body/module.json"),
    ),
    (
        "body/peach_body.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/body/peach_body.json"),
    ),
    (
        "body/peach_body.map2d.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/body/peach_body.map2d.json"),
    ),
    (
        "body/peach_body.patch.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/body/peach_body.patch.json"),
    ),
    (
        "body/body_glow.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/body/body_glow.json"),
    ),
    (
        "body/body_glow.glsl",
        include_bytes!("../../../../../catalog/projects/peach-1d/body/body_glow.glsl"),
    ),
    (
        "leaf/module.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/leaf/module.json"),
    ),
    (
        "leaf/peach_leaf.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/leaf/peach_leaf.json"),
    ),
    (
        "leaf/peach_leaf.map2d.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/leaf/peach_leaf.map2d.json"),
    ),
    (
        "leaf/peach_leaf.patch.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/leaf/peach_leaf.patch.json"),
    ),
    (
        "leaf/leaf_shimmer.json",
        include_bytes!("../../../../../catalog/projects/peach-1d/leaf/leaf_shimmer.json"),
    ),
    (
        "leaf/leaf_shimmer.glsl",
        include_bytes!("../../../../../catalog/projects/peach-1d/leaf/leaf_shimmer.glsl"),
    ),
];

/// `catalog/peach-2d` — the same artwork, the same wiring, the same patch
/// files, declared 2D: `render_2d` planes sampled at the lamps' mapped
/// positions. The pair is the mapping-and-patching evidence — presentation
/// (where the lamps are) and sampling (what asks them for a color) are
/// separate questions.
pub static PEACH_2D_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/clock.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/output.json"),
    ),
    (
        "body/module.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/body/module.json"),
    ),
    (
        "body/peach_body.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/body/peach_body.json"),
    ),
    (
        "body/peach_body.map2d.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/body/peach_body.map2d.json"),
    ),
    (
        "body/peach_body.patch.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/body/peach_body.patch.json"),
    ),
    (
        "body/body_glow.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/body/body_glow.json"),
    ),
    (
        "body/body_glow.glsl",
        include_bytes!("../../../../../catalog/projects/peach-2d/body/body_glow.glsl"),
    ),
    (
        "leaf/module.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/leaf/module.json"),
    ),
    (
        "leaf/peach_leaf.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/leaf/peach_leaf.json"),
    ),
    (
        "leaf/peach_leaf.map2d.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/leaf/peach_leaf.map2d.json"),
    ),
    (
        "leaf/peach_leaf.patch.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/leaf/peach_leaf.patch.json"),
    ),
    (
        "leaf/leaf_shimmer.json",
        include_bytes!("../../../../../catalog/projects/peach-2d/leaf/leaf_shimmer.json"),
    ),
    (
        "leaf/leaf_shimmer.glsl",
        include_bytes!("../../../../../catalog/projects/peach-2d/leaf/leaf_shimmer.glsl"),
    ),
];

/// The gallery's *Examples* section, in order — the demo first, then the
/// single-effect modules.
static EMBEDDED_EXAMPLES: &[EmbeddedExample] = &[
    EmbeddedExample {
        id: crate::STUDIO_DEMO_PROJECT_ID,
        name: "Fyeah Sign",
        kind: "Module",
        files: FYEAH_SIGN_FILES,
    },
    EmbeddedExample {
        id: "catalog/logo-sign",
        name: "Logo Sign",
        kind: "Module",
        files: LOGO_SIGN_FILES,
    },
    EmbeddedExample {
        id: "catalog/plasma",
        name: "Plasma",
        kind: "Module",
        files: PLASMA_FILES,
    },
    EmbeddedExample {
        id: "catalog/meteor",
        name: "Meteor",
        kind: "Module",
        files: METEOR_FILES,
    },
    EmbeddedExample {
        id: "catalog/comet",
        name: "Comet",
        kind: "Module",
        files: COMET_FILES,
    },
    EmbeddedExample {
        id: "catalog/palette-waves",
        name: "Palette Waves",
        kind: "Module",
        files: PALETTE_WAVES_FILES,
    },
    EmbeddedExample {
        id: "catalog/fire2012",
        name: "Fire 2012",
        kind: "Module",
        files: FIRE2012_FILES,
    },
    EmbeddedExample {
        id: "catalog/plasma-duo",
        name: "Plasma Duo",
        kind: "Module",
        files: PLASMA_DUO_FILES,
    },
    EmbeddedExample {
        id: "catalog/zook-dome",
        name: "Zook dome",
        kind: "Module",
        files: ZOOK_DOME_FILES,
    },
    EmbeddedExample {
        id: "catalog/small-dome",
        name: "Small Dome",
        kind: "Module",
        files: SMALL_DOME_FILES,
    },
    EmbeddedExample {
        id: "catalog/peach-1d",
        name: "Peach (1D)",
        kind: "Module",
        files: PEACH_1D_FILES,
    },
    EmbeddedExample {
        id: "catalog/peach-2d",
        name: "Peach (2D)",
        kind: "Module",
        files: PEACH_2D_FILES,
    },
    EmbeddedExample {
        id: "catalog/pulse",
        name: "Pulse",
        kind: "Module",
        files: PULSE_FILES,
    },
    EmbeddedExample {
        id: "catalog/fault-demo",
        name: "Fault demo",
        kind: "Module",
        files: FAULT_DEMO_FILES,
    },
];

/// All embedded examples, gallery order.
pub fn embedded_examples() -> &'static [EmbeddedExample] {
    EMBEDDED_EXAMPLES
}

/// The id prefix every catalog entry carries (`catalog/<slug>`).
pub const CATALOG_ID_PREFIX: &str = "catalog/";

/// The pre-catalog id prefix (`examples/<slug>`), still persisted in
/// user libraries as `SeededFrom { source }` provenance and in the cloud
/// store. Accepted on lookup so those "Remixed from" lines keep resolving;
/// never written anew.
pub const LEGACY_EXAMPLE_ID_PREFIX: &str = "examples/";

/// Look up an embedded example by id. The legacy `examples/<slug>`
/// spelling resolves to the same entry as `catalog/<slug>`.
pub fn embedded_example(id: &str) -> Option<EmbeddedExample> {
    let id = canonical_example_id(id);
    embedded_examples()
        .iter()
        .copied()
        .find(|example| example.id == id)
}

/// Rewrite the legacy `examples/` prefix to `catalog/`; every other id is
/// returned unchanged.
pub fn canonical_example_id(id: &str) -> std::borrow::Cow<'_, str> {
    match id.strip_prefix(LEGACY_EXAMPLE_ID_PREFIX) {
        Some(slug) => std::borrow::Cow::Owned(format!("{CATALOG_ID_PREFIX}{slug}")),
        None => std::borrow::Cow::Borrowed(id),
    }
}

/// Look up an embedded example by its bare slug (the id tail) — the
/// `/p/<slug>` resolution leg. An unknown slug is `None`, never a guess.
pub fn embedded_example_by_slug(slug: &str) -> Option<EmbeddedExample> {
    embedded_examples()
        .iter()
        .copied()
        .find(|example| example.slug() == slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::project::demo_project::DEMO_PROJECT_ID;

    #[test]
    fn demo_example_is_embedded_with_files() {
        let example = embedded_example(DEMO_PROJECT_ID).expect("demo example is embedded");
        assert_eq!(example.name, "Fyeah Sign");
        assert_eq!(example.kind, "Module");
        let files = example.files();
        assert!(
            files
                .iter()
                .any(|(path, _)| path == "project.json" && !files.is_empty())
        );
    }

    #[test]
    fn unknown_example_is_none() {
        assert!(embedded_example("catalog/unknown").is_none());
        assert!(embedded_example("examples/unknown").is_none());
    }

    /// Libraries seeded before the catalog move carry
    /// `SeededFrom { source: "examples/<slug>" }`; that spelling must keep
    /// resolving to the entry now addressed as `catalog/<slug>`.
    #[test]
    fn legacy_example_ids_resolve_to_catalog_entries() {
        let legacy = embedded_example("examples/fyeah-sign").expect("legacy id resolves");
        assert_eq!(legacy.id, "catalog/fyeah-sign");
        assert_eq!(canonical_example_id("examples/plasma"), "catalog/plasma");
        assert_eq!(canonical_example_id("catalog/plasma"), "catalog/plasma");
        assert_eq!(canonical_example_id("prj123"), "prj123");
    }

    /// Bare slugs are the `/p/<slug>` grammar (PD3): every id tail must be
    /// unique or two examples would share an address.
    #[test]
    fn example_slugs_are_unique_id_tails() {
        let mut seen = std::collections::BTreeSet::new();
        for example in embedded_examples() {
            assert_eq!(example.id, format!("catalog/{}", example.slug()));
            assert!(
                seen.insert(example.slug()),
                "duplicate example slug: {}",
                example.slug()
            );
        }
        assert_eq!(
            embedded_example_by_slug("fyeah-sign").map(|e| e.id),
            Some("catalog/fyeah-sign")
        );
        assert!(embedded_example_by_slug("unknown").is_none());
    }

    /// The docs-example contract: plasma-duo drives the SAME shader (and
    /// clock) bytes as the gallery's plasma, so the "What's a shader?"
    /// page's edit-me listing and the standalone example never drift
    /// apart. A plasma shader tweak not copied over breaks this loudly.
    #[test]
    fn plasma_duo_shares_plasmas_shader_bytes() {
        let plasma = embedded_example("catalog/plasma").expect("plasma is embedded");
        let duo = embedded_example("catalog/plasma-duo").expect("plasma-duo is embedded");
        let plasma_files: std::collections::BTreeMap<_, _> = plasma.files().into_iter().collect();
        let duo_files: std::collections::BTreeMap<_, _> = duo.files().into_iter().collect();
        for shared in ["shader.glsl", "shader.json", "clock.json"] {
            assert_eq!(
                plasma_files[&shared.to_string()],
                duo_files[&shared.to_string()],
                "{shared} must stay byte-identical between plasma and plasma-duo"
            );
        }
    }

    /// The mapping-and-patching claim, pinned: the peach's patch documents
    /// say where the lamps land on the wire, which is a fact about the
    /// installation and not about how anything samples them. So the 1D and
    /// 2D peaches — same artwork, opposite declarations — carry the SAME
    /// patch bytes, and the mapping documents they patch against too. A
    /// change to one that is not copied to the other breaks this loudly.
    #[test]
    fn the_two_peaches_share_their_patch_and_mapping_bytes() {
        let one_d = embedded_example("catalog/peach-1d").expect("peach-1d is embedded");
        let two_d = embedded_example("catalog/peach-2d").expect("peach-2d is embedded");
        let one_d_files: std::collections::BTreeMap<_, _> = one_d.files().into_iter().collect();
        let two_d_files: std::collections::BTreeMap<_, _> = two_d.files().into_iter().collect();
        for shared in [
            "body/peach_body.patch.json",
            "leaf/peach_leaf.patch.json",
            "body/peach_body.map2d.json",
            "leaf/peach_leaf.map2d.json",
        ] {
            assert_eq!(
                one_d_files[&shared.to_string()],
                two_d_files[&shared.to_string()],
                "{shared} must stay byte-identical between peach-1d and peach-2d"
            );
        }
    }

    #[test]
    fn every_example_ships_the_two_container_files() {
        // Mitosis (modules.md §1/§6): a package is unopenable without BOTH
        // the container manifest and the root module. Found the hard way
        // when a fixture's mapping document was left out of the demo list.
        for example in embedded_examples() {
            let files = example.files();
            for required in ["project.json", "module.json"] {
                assert!(
                    files.iter().any(|(path, _)| path == required),
                    "{} must ship {required}",
                    example.id
                );
            }
            assert_eq!(
                files.first().map(|(path, _)| path.as_str()),
                Some("project.json"),
                "{} deploys the container manifest first",
                example.id
            );
        }
    }

    #[test]
    fn example_ids_and_names_are_unique() {
        let mut ids: Vec<&str> = embedded_examples().iter().map(|it| it.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "example ids collide");
    }
}
