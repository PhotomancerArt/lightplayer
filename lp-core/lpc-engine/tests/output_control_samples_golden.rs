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
//! **One re-capture, 2026-10-16, and why it is not that.** A shader now
//! compiles in the first frame that renders it instead of one frame later
//! (ADR 2026-08-03-memory-pressure-at-compile-safe-points, amended
//! 2026-10-16). Only what that moves was re-captured, and it was checked to
//! be only that: every example's tick-0 entry (the old black deferral frame,
//! now the lit frame), and the two stateful ones shifted by exactly one frame
//! — meteor's new ticks 0–2 are its old ticks 1–3, and fluid's solver starts
//! one frame sooner. No other tick moved.
//!
//! **And 2026-10-28: small-dome's `out_b` tick 0 only** (project format 12,
//! mapping rebuild M3 P4): the format-6 path places a fixture against its
//! outputs' authored names from the first frame, so the second output no
//! longer publishes one empty frame first. Every other entry, small-dome's
//! included, is unchanged.
//!
//! **And one more, 2026-10-23: fluid only, on purpose.** The fluid solver
//! now takes one velocity pass and red-black, over-relaxed pressure passes
//! instead of ten plain passes per solve, a look change Yona judged side by
//! side and accepted (S3 fluid spike). Only `fluid` was re-captured; every
//! other example's digests are unchanged. The solver's own bits are pinned
//! by `nodes::fluid::solver::tests::output_is_pinned_bit_for_bit`.
//!
//! **And 2026-11-12: every example whose shader has a float constant, for
//! one reason.** These digests run on the host's `lpvm-wasm` backend, which
//! encoded Q32 constants by truncation while the device's `lpvm-native` (and
//! Cranelift) round to nearest (`lps_q32::q32_encode`). The wasm backend now
//! rounds as the device does (one-GLSL-frontend plan, G4), so these are the
//! device's constant bits. Checked to be only that: with the old truncating
//! encode restored and nothing else changed, every previous digest matched.
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
    /// Project directory name, resolved by [`project_dir`] across the
    /// checked-in roots (the root path — and so every output node path
    /// below — is derived from this name alone).
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
                (1446, 0xa23aa7155f6395bb, [0, 0, 15, 150, 86, 156]),
                (1446, 0xe02fba00cfaa0db0, [0, 0, 223, 150, 32, 156]),
                (1446, 0x64e7decf873746d8, [0, 0, 173, 151, 234, 155]),
                (1446, 0x42e3840f4863ef53, [0, 0, 117, 152, 183, 155]),
            ],
        )],
    },
    Expectation {
        project: "basic2",
        outputs: &[(
            "/basic2.show/output.output",
            &[
                (1446, 0xa53d2885db766c67, [255, 255, 0, 0, 18, 51]),
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
                (1446, 0x27758e02da799a8c, [18, 3, 215, 3, 31, 5]),
                (1446, 0x27758e02da799a8c, [18, 3, 215, 3, 31, 5]),
                (1446, 0x27758e02da799a8c, [18, 3, 215, 3, 31, 5]),
                (1446, 0x27758e02da799a8c, [18, 3, 215, 3, 31, 5]),
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
                (720, 0xb0a52ab1abad70ff, [0, 0, 0, 0, 0, 0]),
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
                (1446, 0x0db9dd42ce226b16, [215, 3, 156, 4, 102, 6]),
                (1446, 0x0db9dd42ce226b16, [215, 3, 156, 4, 102, 6]),
                (1446, 0x0db9dd42ce226b16, [215, 3, 156, 4, 102, 6]),
                (1446, 0x0db9dd42ce226b16, [215, 3, 156, 4, 102, 6]),
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
                (12, 0x9c2fa13b0b21b6b4, [255, 255, 95, 66, 90, 66]),
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
                (720, 0x05f9ce5b8c2008f3, [17, 221, 232, 56, 62, 2]),
                (720, 0x6d97f41513df3017, [17, 221, 232, 56, 62, 2]),
                (720, 0x1d16fd99d0729cad, [17, 221, 232, 56, 62, 2]),
                (720, 0xa8179f35da8b2714, [13, 222, 18, 58, 76, 2]),
            ],
        )],
    },
    Expectation {
        project: "fluid",
        outputs: &[(
            "/fluid.show/output.output",
            &[
                (1446, 0x37612fe837d47d09, [0, 0, 0, 0, 0, 0]),
                (1446, 0x37612fe837d47d09, [0, 0, 0, 0, 0, 0]),
                (1446, 0x37612fe837d47d09, [0, 0, 0, 0, 0, 0]),
                (1446, 0xb64a7f20bc1f6a02, [0, 0, 0, 0, 0, 0]),
            ],
        )],
    },
    Expectation {
        project: "meteor",
        outputs: &[(
            "/meteor.show/output.output",
            &[
                (360, 0x95f8d98e7d988c88, [48, 1, 242, 2, 7, 1]),
                (360, 0x5db28013c27c1a03, [26, 1, 187, 2, 244, 0]),
                (360, 0x4b745c99c2148112, [5, 1, 137, 2, 226, 0]),
                (360, 0x88e012a0062357ca, [242, 0, 90, 2, 210, 0]),
            ],
        )],
    },
    // The small-dome: TWO producers scattered across TWO named outputs
    // (many-to-many, format-2 path-identity patches, reversal + rotation).
    // Captured 2026-08-10 as the scatter engine landed (slice-2 P3/P4);
    // re-captured 2026-08-12 when the example's map2d geometry was rebuilt
    // to match the real dome, and again 2026-08-28 when the example (then
    // "mini-dome", a 177-lamp miniature) was rebuilt at FULL scale — 50
    // panels x 119 lamps + a 360-lamp door across 26 ports, generated by
    // `cargo run -p lpt-geodome`. Lamp counts, positions, and patch files
    // all moved; since project format 12 the placements are pinned lamp
    // for lamp by `upgrade_landing` against the format-11 golden.
    // `out_b` published empty on tick 0 until project format 12: output
    // identities self-register on first consume, and the format-2 patch
    // resolved against registered names only, so the named runs settled one
    // tick later. Re-captured 2026-10-28 (mapping rebuild M3 P4), that entry
    // only: a format-6 fixture asks the port tables, which read an output's
    // AUTHORED name before it has registered, so `out_b` lights on tick 0.
    // Ticks 1–3 of both outputs did not move, byte for byte.
    Expectation {
        project: "small-dome",
        outputs: &[
            (
                "/small_dome.show/out_a.output",
                &[
                    (20010, 0xc8819e6f32557fba, [179, 6, 114, 23, 138, 43]),
                    (20010, 0x854a3de61af9a983, [186, 6, 139, 23, 184, 43]),
                    (20010, 0x102b13b31414371a, [193, 6, 164, 23, 232, 43]),
                    (20010, 0x96ebc84625da58cc, [201, 6, 193, 23, 28, 44]),
                ],
            ),
            (
                "/small_dome.show/out_b.output",
                &[
                    (17850, 0x187dd479b963830a, [111, 214, 129, 124, 234, 45]),
                    (17850, 0xa01c00a75c410307, [245, 212, 160, 123, 142, 45]),
                    (17850, 0xd9f8efd2bebdfa2a, [121, 211, 191, 122, 49, 45]),
                    (17850, 0x1eeceeb4a0d72dd2, [237, 209, 212, 121, 211, 44]),
                ],
            ),
        ],
    },
    Expectation {
        project: "palette-waves",
        outputs: &[(
            "/palette_waves.show/output.output",
            &[
                (1446, 0xcdaed69746c3c5be, [199, 142, 230, 97, 221, 35]),
                (1446, 0xdeea542c4974a551, [249, 140, 18, 96, 50, 35]),
                (1446, 0x8bfa071b64716c0b, [53, 139, 70, 94, 138, 34]),
                (1446, 0x25785cdb58417099, [123, 137, 136, 92, 232, 33]),
            ],
        )],
    },
    // The peaches: two fixtures on one output, one of them cut in half and
    // patched in backwards — the multi-producer + format-1-patch case the
    // scatter work (slice 2 P3) must keep byte-identical. Captured
    // 2026-08-10 at e71d3d82b, before any scatter change.
    Expectation {
        project: "peach-1d",
        outputs: &[(
            "/peach_1d.show/output.output",
            &[
                (336, 0xb7dcf4b73a5f4527, [255, 255, 100, 107, 138, 34]),
                (336, 0xa228dc92f49e967b, [255, 255, 48, 106, 43, 34]),
                (336, 0x0e81be85a9678f81, [255, 255, 240, 104, 199, 33]),
                (336, 0xe9db3d4613b596ed, [255, 255, 176, 103, 99, 33]),
            ],
        )],
    },
    Expectation {
        project: "peach-2d",
        outputs: &[(
            "/peach_2d.show/output.output",
            &[
                (336, 0x00c0012b42740a5a, [171, 142, 71, 93, 61, 71]),
                (336, 0xf1b8f8c07e86c5f2, [171, 142, 71, 93, 61, 71]),
                (336, 0x8e7fe3ce0f199147, [172, 142, 72, 93, 61, 71]),
                (336, 0x98812a159f4d665d, [172, 142, 72, 93, 62, 71]),
            ],
        )],
    },
    Expectation {
        project: "perf/baseline",
        outputs: &[(
            "/perf_baseline.show/output.output",
            &[
                (1446, 0xb4411f7bc57d4517, [0, 0, 109, 195, 155, 203]),
                (1446, 0x927e4c04e6829046, [0, 0, 99, 196, 59, 203]),
                (1446, 0x3906147eb74587dc, [0, 0, 83, 197, 219, 202]),
                (1446, 0xeeee2651deb667ab, [0, 0, 64, 198, 129, 202]),
            ],
        )],
    },
    Expectation {
        project: "perf/fastmath",
        outputs: &[(
            "/perf_fastmath.show/output.output",
            &[
                (1446, 0xb4411f7bc57d4517, [0, 0, 109, 195, 155, 203]),
                (1446, 0x927e4c04e6829046, [0, 0, 99, 196, 59, 203]),
                (1446, 0x3906147eb74587dc, [0, 0, 83, 197, 219, 202]),
                (1446, 0xeeee2651deb667ab, [0, 0, 64, 198, 129, 202]),
            ],
        )],
    },
    Expectation {
        project: "plasma",
        outputs: &[(
            "/plasma.show/output.output",
            &[
                (1446, 0x38615cf59effbeee, [168, 255, 104, 66, 120, 66]),
                (1446, 0x04e0fec18c485c41, [177, 255, 75, 67, 149, 65]),
                (1446, 0xef67e135446f2de4, [146, 255, 82, 68, 176, 64]),
                (1446, 0x73a45a2d8f68ee8b, [94, 255, 72, 69, 234, 63]),
            ],
        )],
    },
    Expectation {
        project: "plasma-duo",
        outputs: &[
            (
                "/plasma_duo.show/disc_out.output",
                &[
                    (1446, 0x38615cf59effbeee, [168, 255, 104, 66, 120, 66]),
                    (1446, 0x04e0fec18c485c41, [177, 255, 75, 67, 149, 65]),
                    (1446, 0xef67e135446f2de4, [146, 255, 82, 68, 176, 64]),
                    (1446, 0x73a45a2d8f68ee8b, [94, 255, 72, 69, 234, 63]),
                ],
            ),
            (
                "/plasma_duo.show/grid_out.output",
                &[
                    (1536, 0xf37a641b6633a107, [71, 250, 9, 91, 250, 46]),
                    (1536, 0x66455b9fde42e5fb, [252, 250, 59, 88, 26, 49]),
                    (1536, 0x3187a70635e29491, [189, 251, 46, 85, 111, 51]),
                    (1536, 0xaf85afc4dbf1e553, [104, 252, 104, 82, 145, 53]),
                ],
            ),
        ],
    },
    Expectation {
        project: "rocaille",
        outputs: &[(
            "/rocaille.show/output.output",
            &[
                (1446, 0xf382fcea38a2ba92, [0, 128, 0, 128, 0, 128]),
                (1446, 0x027fea70a9776dd8, [0, 128, 0, 128, 0, 128]),
                (1446, 0x44e8618ac16804bb, [0, 128, 0, 128, 0, 128]),
                (1446, 0x6042aff3dc833167, [0, 128, 0, 128, 0, 128]),
            ],
        )],
    },
    Expectation {
        project: "shader-oracle",
        outputs: &[(
            "/shader_oracle.show/output.output",
            &[
                (384, 0x0c0d70fefe042111, [169, 49, 142, 73, 239, 1]),
                (384, 0x0c0d70fefe042111, [169, 49, 142, 73, 239, 1]),
                (384, 0x0c0d70fefe042111, [169, 49, 142, 73, 239, 1]),
                (384, 0x0c0d70fefe042111, [169, 49, 142, 73, 239, 1]),
            ],
        )],
    },
    Expectation {
        project: "zook-dome",
        outputs: &[(
            "/zook_dome.show/output.output",
            &[
                (9000, 0xe16f9b4088f942a6, [151, 32, 128, 4, 55, 1]),
                (9000, 0xa838b6e0b7ff318c, [58, 33, 149, 4, 57, 1]),
                (9000, 0x6467f68f95afa805, [216, 33, 169, 4, 59, 1]),
                (9000, 0xd179b52d015905ee, [110, 34, 188, 4, 61, 1]),
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

/// Where a golden's project lives: the first checked-in root holding a
/// directory of that name (catalog buckets first, then the test rigs).
fn project_dir(project: &str) -> PathBuf {
    const ROOTS: &[&str] = &["catalog/patterns", "catalog/projects", "projects/test"];
    let workspace = workspace_dir();
    ROOTS
        .iter()
        .map(|root| workspace.join(root).join(project))
        .find(|dir| dir.join("project.json").is_file())
        .unwrap_or_else(|| panic!("{project}: not found under any of {ROOTS:?}"))
}

fn load(project: &str) -> LoadedProjectRuntime {
    let fs = LpFsStd::new(project_dir(project));
    let root = format!("/{}.show", project.replace(['/', '-'], "_"));
    let services = EngineServices::new(TreePath::parse(&root).expect("root path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services)
        .unwrap_or_else(|e| panic!("load {project}: {e:?}"));
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
            .unwrap_or_else(|e| panic!("{project} tick {tick}: {e:?}"));
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
        // The wire form (LE u16 pairs): the golden digests are byte digests
        // and must not move when the engine's storage type does.
        let bytes = buffer.value().bytes();
        let mut head = [0u8; 6];
        for (slot, byte) in head.iter_mut().zip(bytes.iter()) {
            *slot = *byte;
        }
        out.push((entry.path.to_string(), (bytes.len(), fnv1a64(&bytes), head)));
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
    "small-dome",
    "palette-waves",
    "peach-1d",
    "peach-2d",
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
