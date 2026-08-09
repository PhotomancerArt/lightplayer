//! A1 — the golden-buffer oracle for output control samples.
//!
//! Output fragments (P3 of the mapping & patching plan) rewrite the single
//! seam every project's pixels pass through: `OutputNode::consume` stops
//! rendering ONE control product into the whole buffer and starts rendering N
//! producers into disjoint sub-slices of it. Every checked-in example has
//! exactly one producer per output, and for those the new path must be
//! **byte-identical** to the old one.
//!
//! This test is that claim, pinned: load each shipped example, tick it a fixed
//! number of times at a fixed delta, and digest every output node's published
//! runtime buffer after every tick. The expectations below were captured
//! BEFORE any fragment work and must not be edited to make a later change
//! pass — a diff here means the refactor moved pixels, which is the bug the
//! oracle exists to catch.
//!
//! Digest, not raw bytes, because zook-dome alone publishes thousands of
//! lamps; the byte length and the first samples ride along so a failure still
//! says something human ("length changed", "the head is dark now") before
//! pointing at the digest.
//!
//! ```bash
//! cargo test -p lpc-engine --test output_control_samples_golden
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpfs::LpFsStd;

/// Frames each project renders before the buffers are digested.
///
/// More than one because a first frame is special (extents establish, shaders
/// compile) and an animated project's later frames are what prove the clock
/// reached the buffer.
const TICKS: usize = 4;

/// Milliseconds per tick. Fixed so animated examples are reproducible.
const DELTA_MS: u32 = 16;

/// One example project's expected published-output digests.
struct Expectation {
    /// Project directory under `examples/`.
    project: &'static str,
    /// Per-output `(node path, per-tick (byte length, digest, head bytes))`.
    outputs: &'static [(&'static str, &'static [(usize, u64, [u8; 6])])],
}

/// Captured 2026-08-09 on `claude/interesting-yonath-026e15` (post-#399),
/// before any output-fragment change. Regenerate ONLY when a deliberate,
/// separately-argued pixel change lands — never to make a refactor pass.
const GOLDEN: &[Expectation] = &[
    Expectation {
        project: "basic",
        outputs: &[(
            "/basic.show/output.output",
            &[
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0x293433eb0cfc54ee, [0, 0, 223, 150, 32, 156]),
                (1446, 0x46b80f26fcc310db, [0, 0, 173, 151, 234, 155]),
                (1446, 0x97b0f1cb0c786fae, [0, 0, 117, 152, 183, 155]),
            ],
        )],
    },
    Expectation {
        project: "basic2",
        outputs: &[(
            "/basic2.show/output.output",
            &[
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0x62f72a58ec56a560, [255, 255, 0, 0, 140, 55]),
                (1446, 0xf56176d26347a5dd, [255, 255, 0, 0, 60, 60]),
                (1446, 0x48c3934c255ae9ee, [255, 255, 0, 0, 22, 65]),
            ],
        )],
    },
    Expectation {
        project: "button",
        outputs: &[(
            "/button.show/output.output",
            &[
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0xfedb7a2ab9364d91, [18, 3, 215, 3, 30, 5]),
                (1446, 0xfedb7a2ab9364d91, [18, 3, 215, 3, 30, 5]),
                (1446, 0xfedb7a2ab9364d91, [18, 3, 215, 3, 30, 5]),
            ],
        )],
    },
    Expectation {
        project: "button-playlist",
        outputs: &[(
            "/button_playlist.show/output.output",
            &[
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
            ],
        )],
    },
    Expectation {
        project: "comet",
        outputs: &[(
            "/comet.show/output.output",
            &[
                (720, 0xa987b600a8b7fb65, [0, 0, 0, 0, 0, 0]),
                (720, 0x244740d174eb827c, [0, 0, 0, 0, 0, 0]),
                (720, 0x95f86fb6738d6c9a, [198, 0, 220, 0, 200, 7]),
                (720, 0xdf66f16455d365f3, [192, 0, 213, 0, 135, 7]),
            ],
        )],
    },
    Expectation {
        project: "events",
        outputs: &[(
            "/events.show/output.output",
            &[
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0x4bcd0fd1d65e6c5c, [215, 3, 155, 4, 102, 6]),
                (1446, 0x4bcd0fd1d65e6c5c, [215, 3, 155, 4, 102, 6]),
                (1446, 0x4bcd0fd1d65e6c5c, [215, 3, 155, 4, 102, 6]),
            ],
        )],
    },
    Expectation {
        project: "fast",
        outputs: &[(
            "/fast.show/output.output",
            &[
                (6, 0xd7e4fcfa299d713d, [0, 0, 0, 0, 0, 0]),
                (6, 0xd7e4fcfa299d713d, [0, 0, 0, 0, 0, 0]),
                (6, 0xd7e4fcfa299d713d, [0, 0, 0, 0, 0, 0]),
                (6, 0x9111c5600580fdaf, [2, 0, 0, 0, 0, 0]),
            ],
        )],
    },
    Expectation {
        project: "fiber-headband",
        outputs: &[(
            "/fiber_headband.show/output.output",
            &[
                (12, 0x5467b0da1d106495, [0, 0, 0, 0, 0, 0]),
                (12, 0xbe4fd8207739fa10, [252, 255, 176, 67, 11, 65]),
                (12, 0xe5309614e0f72af2, [243, 255, 3, 69, 191, 63]),
                (12, 0x93c382dffe63cc08, [229, 255, 84, 70, 118, 62]),
            ],
        )],
    },
    Expectation {
        project: "fire2012",
        outputs: &[(
            "/fire2012.show/output.output",
            &[
                (720, 0xa987b600a8b7fb65, [0, 0, 0, 0, 0, 0]),
                (720, 0x168f131e35222c7e, [17, 221, 232, 56, 62, 2]),
                (720, 0xe014534788eb6176, [17, 221, 232, 56, 62, 2]),
                (720, 0x9a0a8cbc88c73767, [13, 222, 18, 58, 76, 2]),
            ],
        )],
    },
    Expectation {
        project: "fluid",
        outputs: &[(
            "/fluid.show/output.output",
            &[
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0x723bf9822969d790, [0, 0, 0, 0, 0, 0]),
            ],
        )],
    },
    Expectation {
        project: "meteor",
        outputs: &[(
            "/meteor.show/output.output",
            &[
                (360, 0x9cccbb9b79c47545, [0, 0, 0, 0, 0, 0]),
                (360, 0x5b336d7f0bd8288b, [48, 1, 242, 2, 7, 1]),
                (360, 0x6d217051e716f19d, [26, 1, 187, 2, 244, 0]),
                (360, 0x3accd4b816ddb8b1, [5, 1, 137, 2, 226, 0]),
            ],
        )],
    },
    Expectation {
        project: "palette-waves",
        outputs: &[(
            "/palette_waves.show/output.output",
            &[
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0xe054737b58c4a061, [249, 140, 18, 96, 50, 35]),
                (1446, 0xe8e6c2b5534ae66a, [53, 139, 70, 94, 138, 34]),
                (1446, 0xf31c7a419567abd8, [123, 137, 136, 92, 232, 33]),
            ],
        )],
    },
    Expectation {
        project: "perf/baseline",
        outputs: &[(
            "/perf_baseline.show/output.output",
            &[
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0xbfca2fe59e38ed4f, [0, 0, 99, 196, 58, 203]),
                (1446, 0x8f4bf29c18181b63, [0, 0, 83, 197, 218, 202]),
                (1446, 0x0ca30df76e1c7394, [0, 0, 64, 198, 128, 202]),
            ],
        )],
    },
    Expectation {
        project: "perf/fastmath",
        outputs: &[(
            "/perf_fastmath.show/output.output",
            &[
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0xbfca2fe59e38ed4f, [0, 0, 99, 196, 58, 203]),
                (1446, 0x8f4bf29c18181b63, [0, 0, 83, 197, 218, 202]),
                (1446, 0x0ca30df76e1c7394, [0, 0, 64, 198, 128, 202]),
            ],
        )],
    },
    Expectation {
        project: "plasma",
        outputs: &[(
            "/plasma.show/output.output",
            &[
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0x9207f21355b6ba3e, [177, 255, 75, 67, 149, 65]),
                (1446, 0x3c1bf1d3dc5cb69d, [146, 255, 82, 68, 176, 64]),
                (1446, 0xe7c16df42bdd3a00, [94, 255, 72, 69, 234, 63]),
            ],
        )],
    },
    Expectation {
        project: "plasma-duo",
        outputs: &[
            (
                "/plasma_duo.show/disc_out.output",
                &[
                    (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                    (1446, 0x9207f21355b6ba3e, [177, 255, 75, 67, 149, 65]),
                    (1446, 0x3c1bf1d3dc5cb69d, [146, 255, 82, 68, 176, 64]),
                    (1446, 0xe7c16df42bdd3a00, [94, 255, 72, 69, 234, 63]),
                ],
            ),
            (
                "/plasma_duo.show/grid_out.output",
                &[
                    (1536, 0xb3664ba8bd45e0e0, [71, 250, 9, 91, 250, 46]),
                    (1536, 0xa4c4ed2e0580fbdf, [252, 250, 59, 88, 26, 49]),
                    (1536, 0xa09e5e4851c05cdb, [189, 251, 46, 85, 111, 51]),
                    (1536, 0xf09f97b77ef1b673, [104, 252, 104, 82, 145, 53]),
                ],
            ),
        ],
    },
    Expectation {
        project: "rocaille",
        outputs: &[(
            "/rocaille.show/output.output",
            &[
                (1446, 0x5ff65547467f0dbd, [0, 0, 0, 0, 0, 0]),
                (1446, 0x47365bf5f6574c17, [0, 128, 0, 128, 0, 128]),
                (1446, 0x92ae57178851ac70, [0, 128, 0, 128, 0, 128]),
                (1446, 0xf17267e6aed9c733, [0, 128, 0, 128, 0, 128]),
            ],
        )],
    },
    Expectation {
        project: "shader-oracle",
        outputs: &[(
            "/shader_oracle.show/output.output",
            &[
                (384, 0xc86ec345c0ee8125, [0, 0, 0, 0, 0, 0]),
                (384, 0x142d97adaf4a30b8, [169, 49, 143, 73, 240, 1]),
                (384, 0x142d97adaf4a30b8, [169, 49, 143, 73, 240, 1]),
                (384, 0x142d97adaf4a30b8, [169, 49, 143, 73, 240, 1]),
            ],
        )],
    },
    Expectation {
        project: "zook-dome",
        outputs: &[(
            "/zook_dome.show/output.output",
            &[
                (9000, 0x2198935831931845, [0, 0, 0, 0, 0, 0]),
                (9000, 0x836afb177d66a20b, [58, 33, 149, 4, 57, 1]),
                (9000, 0x762067a8d5150dc5, [215, 33, 169, 4, 59, 1]),
                (9000, 0xbca3930e3c5a68bc, [109, 34, 188, 4, 61, 1]),
            ],
        )],
    },
];

#[test]
fn shipped_examples_publish_identical_output_bytes() {
    let mut failures = Vec::new();
    for expectation in GOLDEN {
        let actual = render_project(expectation.project);
        let expected: OutputDigests = expectation
            .outputs
            .iter()
            .map(|(path, ticks)| ((*path).to_string(), ticks.to_vec()))
            .collect();
        if actual != expected {
            failures.push(format!(
                "{}: published output bytes moved. Actual:\n{}",
                expectation.project,
                render_literal(expectation.project, &actual),
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Coverage, so a new example cannot quietly escape the oracle: every project
/// in [`EXAMPLES`] must carry a captured expectation.
#[test]
fn every_listed_example_is_captured() {
    let missing: Vec<&str> = EXAMPLES
        .iter()
        .copied()
        .filter(|project| {
            !GOLDEN
                .iter()
                .any(|expectation| expectation.project == *project)
        })
        .collect();

    assert!(
        missing.is_empty(),
        "no captured output bytes for {missing:?} — run `cargo test -p lpc-engine \
         --test output_control_samples_golden -- --ignored --nocapture` and paste \
         the printed table",
    );
}

type OutputDigests = Vec<(String, Vec<(usize, u64, [u8; 6])>)>;

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

fn load(project: &str) -> LoadedProjectRuntime {
    let fs = LpFsStd::new(workspace_dir().join("examples").join(project));
    let root = format!("/{}.show", project.replace(['/', '-'], "_"));
    let services = EngineServices::new(TreePath::parse(&root).expect("root path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services)
        .unwrap_or_else(|e| panic!("load examples/{project}: {e:?}"));
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    rt
}

/// Tick `project` and digest every output node's published buffer per tick.
fn render_project(project: &str) -> OutputDigests {
    let mut rt = load(project);
    let mut per_output: OutputDigests = Vec::new();
    for tick in 0..TICKS {
        rt.tick(DELTA_MS)
            .unwrap_or_else(|e| panic!("examples/{project} tick {tick}: {e:?}"));
        for (path, digest) in published_outputs(&rt) {
            match per_output.iter_mut().find(|(known, _)| known == &path) {
                Some((_, ticks)) => ticks.push(digest),
                None => per_output.push((path, vec![digest])),
            }
        }
    }
    per_output.sort_by(|a, b| a.0.cmp(&b.0));
    per_output
}

/// `(node path, (byte length, digest, head bytes))` for every output node.
fn published_outputs(rt: &LoadedProjectRuntime) -> Vec<(String, (usize, u64, [u8; 6]))> {
    let engine = rt.engine();
    let mut out = Vec::new();
    for entry in engine.tree().entries() {
        let Some(buffer_id) = engine.runtime_output_sink_buffer_id(entry.id) else {
            continue;
        };
        let Some(buffer) = engine.runtime_buffers().get(buffer_id) else {
            continue;
        };
        let bytes = &buffer.value().bytes;
        let mut head = [0u8; 6];
        for (slot, byte) in head.iter_mut().zip(bytes.iter()) {
            *slot = *byte;
        }
        out.push((entry.path.to_string(), (bytes.len(), fnv1a64(bytes), head)));
    }
    out
}

/// FNV-1a, 64-bit: a stable, dependency-free digest for buffer identity.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The `Expectation` literal for what a project actually published — printed
/// on failure so a deliberate recapture is a copy, not a transcription.
fn render_literal(project: &str, actual: &OutputDigests) -> String {
    let mut text = String::new();
    text.push_str(&format!(
        "    Expectation {{\n        project: {project:?},\n        outputs: &[\n"
    ));
    for (path, ticks) in actual {
        text.push_str(&format!("            ({path:?}, &[\n"));
        for (len, digest, head) in ticks {
            text.push_str(&format!(
                "                ({len}, 0x{digest:016x}, {head:?}),\n"
            ));
        }
        text.push_str("            ]),\n");
    }
    text.push_str("        ],\n    },\n");
    text
}

/// Every example directory that carries a project — the capture list.
///
/// A new example must be added here, and the capture regenerated, or it goes
/// unguarded. `perf` holds two projects in sub-directories rather than one at
/// its root, which is why it is spelled out.
///
/// The three ESP-NOW examples (`button-sign`, `fyeah-button`, `fyeah-sign`)
/// are absent: their control-radio node refuses to tick without a radio
/// service, which a bare host engine has none of. Their output stage is the
/// same fixture→output chain the rest of the list already pins.
const EXAMPLES: &[&str] = &[
    "basic",
    "basic2",
    "button",
    "button-playlist",
    "comet",
    "events",
    "fast",
    "fiber-headband",
    "fire2012",
    "fluid",
    "meteor",
    "palette-waves",
    "perf/baseline",
    "perf/fastmath",
    "plasma",
    "plasma-duo",
    "rocaille",
    "shader-oracle",
    "zook-dome",
];

/// Capture helper: prints the `GOLDEN` table for every example in
/// [`EXAMPLES`]. Ignored by default — run it explicitly (`--ignored
/// --nocapture`) when the table is being (re)captured on purpose.
#[test]
#[ignore = "capture helper, not an assertion"]
fn capture_golden_table() {
    let mut text = String::from("const GOLDEN: &[Expectation] = &[\n");
    for project in EXAMPLES {
        text.push_str(&render_literal(project, &render_project(project)));
    }
    text.push_str("];\n");
    println!("{text}");
}
