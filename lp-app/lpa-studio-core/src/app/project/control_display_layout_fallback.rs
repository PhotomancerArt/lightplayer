//! Client-side display-layout synthesis for map2d fixtures.
//!
//! The engine refuses to send a display layout that would not fit one
//! project-read frame (`DISPLAY_LAYOUT_WIRE_BUDGET`), answering
//! `ControlDisplayLayoutProbeResult::Unsupported` instead — otherwise the
//! oversized event wedges the whole read stream (see
//! `docs/defects/2026-08-04-oversized-display-layout-wedges-project-read.md`).
//! A dome-scale fixture (1500 lamps) lands there, which left both the
//! fixture face and the module output face with "Control product has no
//! display layout".
//!
//! When the producing node is a fixture whose mapping is a `Map2d` document
//! the Studio already holds, the layout is derivable client-side: the
//! document plus the fixture's render extent is exactly what the engine
//! resolves from. This module does that derivation, and it does it through
//! the SAME code the engine runs so the two can never drift:
//!
//! - [`lpc_mapping::resolve`] + [`lpc_mapping::fit_points`] produce the
//!   positions (mirroring `mapping_from_map2d_doc` in
//!   `lpc-engine/src/nodes/fixture/mapping/map2d.rs`, which builds the same
//!   carrier);
//! - [`generate_mapping_points`] — lpc-model's shared generator, the very
//!   function the engine's `fixture_control_layout_2d` calls — turns the
//!   carrier into centers, radii, and channels;
//! - the lamp/path construction mirrors `fixture_control_layout_2d` and
//!   `fixture_path_spans` (`lpc-engine/src/nodes/fixture/fixture_node.rs`).
//!
//! Only the carrier assembly is mirrored rather than shared: it is engine
//! code today and `lpc-mapping` cannot reach `lpc-model`'s carrier type.
//! `synthesized_map2d_layout_matches_the_engines_probe_layout` in
//! `studio_face_e2e_tests` is the oracle that keeps the mirror honest.

use lpc_mapping::{Map2dDoc, ResolvedMap2d, fit_points, resolve};
use lpc_model::nodes::fixture::{ResolvedMappingCompact, ResolvedSpan, generate_mapping_points};
use lpc_model::{ControlLamp2d, ControlLayout2d, ControlPathSpan2d, Revision};

/// Synthesize the display layout a `width` × `height` fixture would publish
/// for `doc`, stamped with `revision`.
///
/// `None` when the document cannot be resolved or fitted — the caller keeps
/// the engine's answer (no layout) rather than showing invented geometry.
pub(crate) fn synthesized_map2d_layout(
    doc: &Map2dDoc,
    revision: Revision,
    width: u32,
    height: u32,
) -> Option<ControlLayout2d> {
    let mapping = compact_mapping_from_doc(doc, width, height)?;
    // Mirrors `fixture_control_layout_2d`: one lamp per mapping point, in
    // the generator's visit order, with the sample offset derived from the
    // channel exactly as the engine derives it.
    let lamps = generate_mapping_points(&mapping, width, height)
        .into_iter()
        .map(|point| ControlLamp2d {
            lamp_index: point.channel,
            sample_start: point.channel.saturating_mul(3),
            center: point.center,
            radius: point.radius,
        })
        .collect();
    // Mirrors `fixture_path_spans`' `Compact` arm: one span per physical
    // strand that actually has lamps, in channel-assignment order — a
    // rotational `repeat` object contributes one per instance.
    let paths = mapping
        .spans
        .iter()
        .filter(|span| span.count > 0)
        .map(|span| ControlPathSpan2d {
            first_lamp: span.first_channel,
            lamp_count: span.count,
        })
        .collect();
    Some(ControlLayout2d::new(revision, width, height, lamps).with_paths(paths))
}

/// Synthesize a project's display layout from its PACKAGE FILES.
///
/// The lens path reaches the same synthesis through the live node tree and
/// the asset-body cache ([`ProjectController::apply_synthesized_display_layouts`]),
/// both of which exist only for the project the editor is open on. The
/// device card's frame feed has neither: it pulls frames off a board the
/// lens is not on, and the only local copy of that board's project is the
/// library package connect-as-pull banked (D8). So this entry point takes
/// the package as `(path, bytes)` — exactly `LibraryStore` handles' own
/// `read_all_files` shape — and reads the two facts the synthesis needs out
/// of it: the fixture's render extent and its map2d document.
///
/// One fixture answers, deliberately: the card follows ONE output node, and
/// a project driving several fixtures has no single picture to show on a
/// card anyway. Files are visited in the caller's order (the library hands
/// them over sorted), so the answer is stable across pulls.
///
/// `None` for everything that is not a map2d fixture project — an
/// unreadable document, a non-map2d mapping, a missing source file. The
/// caller keeps the device's answer (no layout) rather than inventing
/// geometry.
///
/// [`ProjectController::apply_synthesized_display_layouts`]: crate::ProjectController
pub(crate) fn synthesized_layout_from_package_files(
    files: &[(String, Vec<u8>)],
    revision: Revision,
) -> Option<ControlLayout2d> {
    let (fixture_path, fixture) = files.iter().find_map(|(path, bytes)| {
        let doc: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let kind = doc.get("kind")?.as_str()?;
        kind.eq_ignore_ascii_case("fixture").then_some((path, doc))
    })?;
    let mapping = fixture.get("mapping")?;
    if !mapping.get("kind")?.as_str()?.eq_ignore_ascii_case("map2d") {
        return None;
    }
    let source = mapping.get("source")?.as_str()?;
    let render_size = fixture.get("render_size")?;
    let width = u32::try_from(render_size.get("width")?.as_u64()?).ok()?;
    let height = u32::try_from(render_size.get("height")?.as_u64()?).ok()?;

    let source_path = sibling_path(fixture_path, source);
    let (_, document) = files
        .iter()
        .find(|(path, _)| path.trim_start_matches('/') == source_path)?;
    let doc = Map2dDoc::from_json(core::str::from_utf8(document).ok()?).ok()?;
    synthesized_map2d_layout(&doc, revision, width, height)
}

/// Resolve a node's asset reference against the node file's own directory,
/// the way the runtime resolves it. A bare name next to `fixture.json` is
/// the overwhelmingly common form; a path with separators is taken as
/// package-relative.
fn sibling_path(node_path: &str, source: &str) -> String {
    let source = source.trim_start_matches('/');
    if source.contains('/') {
        return source.to_string();
    }
    match node_path.trim_start_matches('/').rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{source}"),
        None => source.to_string(),
    }
}

/// The engine's `mapping_from_map2d_doc`, mirrored: resolve the document in
/// doc space, then aspect-fit the positions into the fixture's render
/// extent. An empty document yields an empty carrier without a fit call —
/// `fit_points` rejects empty bounds.
fn compact_mapping_from_doc(
    doc: &Map2dDoc,
    width: u32,
    height: u32,
) -> Option<ResolvedMappingCompact> {
    let ResolvedMap2d { lamps, spans } = resolve(doc).ok()?;
    if lamps.is_empty() {
        return Some(ResolvedMappingCompact {
            spans: Vec::new(),
            points: Vec::new(),
            sample_diameter: doc.sample_diameter,
        });
    }

    let positions: Vec<[f32; 2]> = lamps.iter().map(|lamp| lamp.pos).collect();
    drop(lamps);
    let points = fit_points(&positions, doc.canvas_bounds(), width, height).ok()?;

    let compact_spans = spans
        .iter()
        .map(|span| ResolvedSpan {
            object: span.object,
            // Wiring order is channel order: an object's first lamp index
            // IS its first channel.
            first_channel: span.start,
            count: span.count,
        })
        .collect();
    Some(ResolvedMappingCompact {
        spans: compact_spans,
        points,
        sample_diameter: doc.sample_diameter,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `rows` × `cols` grid document, the smallest map2d shape that
    /// exercises spans, centers, and radii together.
    fn grid_doc(objects: &[(&str, u32, u32)]) -> Map2dDoc {
        let mut json = String::from("{\"format\": 1, \"sample_diameter\": 2.0, \"objects\": [");
        for (index, (name, cols, rows)) in objects.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "{{\"name\": \"{name}\", \"shape\": {{\"grid\": {{\"origin\": [0, 0], \
                 \"cols\": {cols}, \"rows\": {rows}, \"pitch\": 10}}}}}}"
            ));
        }
        json.push_str("]}");
        Map2dDoc::from_json(&json).expect("grid document parses")
    }

    #[test]
    fn synthesized_layout_numbers_lamps_and_samples_in_wiring_order() {
        let doc = grid_doc(&[("a", 2, 2), ("b", 3, 1)]);

        let layout = synthesized_map2d_layout(&doc, Revision::new(7), 16, 16)
            .expect("grid document synthesizes");

        assert_eq!(layout.revision, Revision::new(7));
        assert_eq!((layout.width_hint, layout.height_hint), (16, 16));
        assert_eq!(layout.lamps.len(), 7);
        for (index, lamp) in layout.lamps.iter().enumerate() {
            assert_eq!(lamp.lamp_index, index as u32, "lamp index at {index}");
            assert_eq!(
                lamp.sample_start,
                index as u32 * 3,
                "sample start at {index}"
            );
            assert!((0.0..=1.0).contains(&lamp.center[0]), "x at {index}");
            assert!((0.0..=1.0).contains(&lamp.center[1]), "y at {index}");
        }
        // Radius mirrors `normalized_sample_radius`: half the diameter over
        // the larger texture dimension.
        assert!((layout.lamps[0].radius - (2.0 / 2.0) / 16.0).abs() < 1e-6);
        assert_eq!(
            layout
                .paths
                .iter()
                .map(|span| (span.first_lamp, span.lamp_count))
                .collect::<Vec<_>>(),
            vec![(0, 4), (4, 3)],
            "one span per object, in wiring order"
        );
    }

    /// A repeated object is many strands: the synthesized layout must publish
    /// one path span per instance, exactly as the engine's carrier does, or
    /// the client-side preview would draw one long strand where the fixture
    /// has five.
    #[test]
    fn a_repeated_object_synthesizes_one_path_span_per_instance() {
        let doc = lpc_mapping::corpus::repeated_sector();

        let layout = synthesized_map2d_layout(&doc, Revision::new(2), 64, 64)
            .expect("repeated document synthesizes");

        assert_eq!(doc.objects.len(), 1, "one authored object");
        assert_eq!(layout.lamps.len(), 60);
        assert_eq!(
            layout
                .paths
                .iter()
                .map(|span| (span.first_lamp, span.lamp_count))
                .collect::<Vec<_>>(),
            vec![(0, 12), (12, 12), (24, 12), (36, 12), (48, 12)],
        );
        // Lamp numbering still runs straight through the instances.
        for (index, lamp) in layout.lamps.iter().enumerate() {
            assert_eq!(lamp.lamp_index, index as u32);
            assert_eq!(lamp.sample_start, index as u32 * 3);
        }
    }

    #[test]
    fn radius_normalizes_against_the_larger_render_dimension() {
        let doc = grid_doc(&[("a", 2, 2)]);

        let layout = synthesized_map2d_layout(&doc, Revision::new(1), 200, 100)
            .expect("grid document synthesizes");

        assert!((layout.lamps[0].radius - 0.005).abs() < 1e-6);
    }

    /// The non-lens entry point: a package as the library holds it (node
    /// docs + the mapping document next to them) synthesizes the same
    /// layout the document-level call does.
    #[test]
    fn package_files_synthesize_the_fixtures_layout() {
        let files = package_files(
            "fixture.json",
            r#"{"kind": "Fixture", "render_size": {"width": 16, "height": 16},
                "mapping": {"kind": "Map2d", "source": "fixture.map2d.json"}}"#,
            "fixture.map2d.json",
        );

        let layout = synthesized_layout_from_package_files(&files, Revision::new(5))
            .expect("a map2d fixture package synthesizes");

        let direct = synthesized_map2d_layout(&grid_doc(&[("a", 2, 2)]), Revision::new(5), 16, 16)
            .expect("the same document synthesizes directly");
        assert_eq!(layout, direct);
    }

    /// A node doc that names its mapping in a subdirectory resolves against
    /// the package root, not the node's directory.
    #[test]
    fn a_pathed_mapping_source_resolves_package_relative() {
        let mut files = package_files(
            "nodes/fixture.json",
            r#"{"kind": "Fixture", "render_size": {"width": 16, "height": 16},
                "mapping": {"kind": "Map2d", "source": "maps/dome.map2d.json"}}"#,
            "maps/dome.map2d.json",
        );
        files.sort_by(|a, b| a.0.cmp(&b.0));

        assert!(synthesized_layout_from_package_files(&files, Revision::new(1)).is_some());
    }

    /// Everything that is not a map2d fixture leaves the device's answer
    /// alone — the fallback never invents geometry.
    #[test]
    fn a_project_without_a_map2d_fixture_synthesizes_nothing() {
        let shader = vec![("shader.json".to_string(), br#"{"kind": "Shader"}"#.to_vec())];
        assert!(synthesized_layout_from_package_files(&shader, Revision::new(1)).is_none());

        // A fixture whose mapping is not a map2d document.
        let strip = vec![(
            "fixture.json".to_string(),
            br#"{"kind": "Fixture", "render_size": {"width": 8, "height": 8},
                 "mapping": {"kind": "Strip"}}"#
                .to_vec(),
        )];
        assert!(synthesized_layout_from_package_files(&strip, Revision::new(1)).is_none());

        // A map2d fixture whose document was never packaged.
        let starved = vec![(
            "fixture.json".to_string(),
            br#"{"kind": "Fixture", "render_size": {"width": 8, "height": 8},
                 "mapping": {"kind": "Map2d", "source": "gone.map2d.json"}}"#
                .to_vec(),
        )];
        assert!(synthesized_layout_from_package_files(&starved, Revision::new(1)).is_none());
    }

    /// A package with node docs, one map2d document, and the non-JSON
    /// clutter (README, shader source) a real package carries.
    fn package_files(
        fixture_path: &str,
        fixture_json: &str,
        map_path: &str,
    ) -> Vec<(String, Vec<u8>)> {
        let doc = grid_doc(&[("a", 2, 2)]);
        vec![
            ("README.md".to_string(), b"not json".to_vec()),
            (
                "project.json".to_string(),
                br#"{"format": 8, "name": "test"}"#.to_vec(),
            ),
            (fixture_path.to_string(), fixture_json.as_bytes().to_vec()),
            (map_path.to_string(), doc.to_json().into_bytes()),
        ]
    }

    #[test]
    fn empty_document_synthesizes_an_empty_layout() {
        let doc = Map2dDoc::new();

        let layout = synthesized_map2d_layout(&doc, Revision::new(3), 16, 16)
            .expect("an empty document is a valid empty layout, not a failure");

        assert!(layout.lamps.is_empty());
        assert!(layout.paths.is_empty());
        assert_eq!((layout.width_hint, layout.height_hint), (16, 16));
    }
}
