//! Generator for `examples/logo-sign/sign.map2d.json` — the brand artwork as
//! a real, someday-buildable LED piece.
//!
//! The Logo Sign is the landing hero made physical: a shaped PCB matrix in the
//! outline of the brand's play triangle, and "LightPlayer" beneath it as a
//! string of single-stroke letter strands. Both live on ONE mapping canvas,
//! which is the hero's stage rect — so the shader that fills the triangle
//! flows on through the letters, exactly as the hero says it does.
//!
//! The document is **generated, not drawn**: the triangle outline comes from
//! [`fillet_tri_corners`] — the same construction the mark, the favicon and
//! the hero clip path are built from — and the letters come from the committed
//! [`letters.svg`](../../../../../examples/logo-sign/letters.svg) through the
//! corpus SVG importer. Drawing it by hand would have forked the brand
//! geometry the moment either side moved.
//!
//! Drift is gated the way the favicon's is: [`logo_sign_map2d_in_sync`]
//! compares the committed JSON byte for byte, and
//! [`logo_sign_map2d_regen`] rewrites it. A nudge to `HERO_TRI`, to
//! `letters.svg`, or to the lattice pitch fails the first with instructions to
//! run the second.
//!
//! Test-only on purpose (`#[cfg(test)]` at the module declaration): nothing in
//! the running app reads this, and the wasm bundle should not carry the letter
//! SVG twice.

use lpc_mapping::{
    FilledPolygonShape, GridCorner, GridRouting, Map2dDoc, Map2dObject, Map2dShape, PathShape,
};

use crate::app::home::brand_hero::{
    HERO_BOX, HERO_CORNER_RATIO, HERO_TRI, STAGE, WORD_BASELINE_Y, WORD_PX,
};
use crate::base::logo_mark::fillet_tri_corners;

/// The committed letter skeletons. Authored in the corpus SVG subset (one
/// top-level group per letter, one straight-line polyline, one
/// `path:N,count:N` label); see the file's own header for the em frame.
const LETTERS_SVG: &str = include_str!("../../../../../examples/logo-sign/letters.svg");

/// The letters, in wiring order, with the segment indices of each letter's
/// jumper wire — the pen-lifts a single stroke cannot reach.
///
/// The SVG carries the jumper as a real segment (that IS the wire); marking
/// it inert is what keeps it from carrying lamps. Three letters need one:
/// the dot of the `i`, the crossbar of the `t`, the left arm of the `y`.
/// Every other letter is one continuous stroke, and the `h` retraces its
/// ascender rather than lift.
const LETTERS: [(&str, &[u32]); 11] = [
    ("L", &[]),
    ("i", &[1]),
    ("g", &[]),
    ("h", &[]),
    ("t", &[1]),
    ("P", &[]),
    ("l", &[]),
    ("a", &[]),
    ("y", &[1]),
    ("e", &[]),
    ("r", &[]),
];

/// The letter SVG's em box and baseline, in its own units. The generator
/// maps this frame onto the hero's wordmark metrics, so the letters land
/// where the hero's clip glyphs do.
const LETTER_EM: f32 = 100.0;
const LETTER_BASELINE: f32 = 100.0;

/// Lattice spacing of the triangle matrix, in doc units.
///
/// Doc units are arbitrary, so the number that means something is the one it
/// derives: 11.5 lays 16 lattice rows across the triangle and populates 132
/// cells — the ~130–140 LED board the vision asks for.
/// [`logo_sign_lamp_counts_are_pinned`] holds both ends of that sentence.
const TRI_PITCH: f32 = 11.5;

/// Segments per corner fillet. The arcs are ~120° each and a lamp lattice
/// samples the outline at `TRI_PITCH`, so eight chords per corner is already
/// finer than anything the board can show.
const TRI_ARC_SEGMENTS: usize = 8;

/// Lamp sample diameter, doc units. Between the letters' stroke spacing
/// (~6) and the matrix pitch (11.5): dots that read as dots on both.
const SAMPLE_DIAMETER: f32 = 5.0;

/// The Logo Sign mapping document: the triangle matrix, then the eleven
/// letter strands left to right. Object order is wiring order.
fn logo_sign_map2d() -> Map2dDoc {
    let mut objects = vec![triangle_object()];
    objects.extend(letter_objects());
    let mut doc = Map2dDoc {
        sample_diameter: SAMPLE_DIAMETER,
        // The hero's stage rect: the canvas is how that framing survives into
        // the editor, and doc units are arbitrary otherwise.
        canvas: Some([0.0, 0.0, STAGE.0, STAGE.1]),
        objects,
        ..Map2dDoc::new()
    };
    doc.normalize_format();
    doc
}

/// The committed document's bytes: pretty JSON with a trailing newline, the
/// same shape every generated map2d in `examples/` is written in.
fn logo_sign_map2d_json() -> String {
    logo_sign_map2d().to_json_pretty() + "\n"
}

/// Object 1: the shaped matrix. The brand triangle in stage coordinates,
/// flattened to an outline a lamp lattice can be poured into.
fn triangle_object() -> Map2dObject {
    let (cx, cy, r) = stage_triangle();
    let rho = r * HERO_CORNER_RATIO;
    let mut points = Vec::new();
    for [enter, leave] in fillet_tri_corners(cx, cy, r, rho) {
        points.extend(fillet_arc(enter, leave, rho, (cx, cy)));
    }
    Map2dObject {
        name: "matrix".to_string(),
        id: None,
        stride: None,
        shape: Map2dShape::FilledPolygon(FilledPolygonShape {
            points,
            pitch: TRI_PITCH,
            angle_deg: 0.0,
            origin: [0.0, 0.0],
            routing: GridRouting::Snake,
            start_corner: GridCorner::Tl,
        }),
    }
}

/// The brand triangle in stage coordinates: the hero centers its triangle
/// window in the stage, so the window's own offset is what moves it.
fn stage_triangle() -> (f32, f32, f32) {
    let (cx, cy, r) = HERO_TRI;
    ((STAGE.0 - HERO_BOX.0) / 2.0 + cx, cy, r)
}

/// One corner fillet as chords: the arc of radius `rho` from `enter` to
/// `leave`, faceted into [`TRI_ARC_SEGMENTS`] segments, endpoints included.
///
/// The arc center is the one of the two circles through both tangent points
/// that sits on `toward`'s side — the triangle's own center, i.e. inside the
/// outline, which is where a fillet's center always is.
fn fillet_arc(enter: (f32, f32), leave: (f32, f32), rho: f32, toward: (f32, f32)) -> Vec<[f32; 2]> {
    let mid = ((enter.0 + leave.0) * 0.5, (enter.1 + leave.1) * 0.5);
    let chord = (leave.0 - enter.0, leave.1 - enter.1);
    let chord_len = (chord.0 * chord.0 + chord.1 * chord.1).sqrt();
    let half = chord_len * 0.5;
    let height = (rho * rho - half * half).max(0.0).sqrt();
    let normal = (-chord.1 / chord_len, chord.0 / chord_len);
    let near = (mid.0 + normal.0 * height, mid.1 + normal.1 * height);
    let far = (mid.0 - normal.0 * height, mid.1 - normal.1 * height);
    let dist2 = |p: (f32, f32)| (p.0 - toward.0).powi(2) + (p.1 - toward.1).powi(2);
    let center = if dist2(near) < dist2(far) { near } else { far };

    let start = (enter.1 - center.1).atan2(enter.0 - center.0);
    let end = (leave.1 - center.1).atan2(leave.0 - center.0);
    // The shorter way round: a fillet on a 60° corner turns 120°, never a
    // half turn, so the wrapped difference is unambiguous.
    let mut sweep = end - start;
    let turn = std::f32::consts::TAU;
    while sweep <= -std::f32::consts::PI {
        sweep += turn;
    }
    while sweep > std::f32::consts::PI {
        sweep -= turn;
    }
    (0..=TRI_ARC_SEGMENTS)
        .map(|step| {
            let angle = start + sweep * (step as f32 / TRI_ARC_SEGMENTS as f32);
            round_point([center.0 + rho * angle.cos(), center.1 + rho * angle.sin()])
        })
        .collect()
}

/// Objects 2..12: the letter strands, imported from the committed SVG and
/// placed on the hero's wordmark metrics.
fn letter_objects() -> Vec<Map2dObject> {
    let imported = lpc_mapping::import::svg_to_doc(LETTERS_SVG, SAMPLE_DIAMETER)
        .expect("examples/logo-sign/letters.svg parses as a corpus mapping SVG");
    let [_, _, svg_width, _] = imported
        .canvas
        .expect("letters.svg declares a viewBox, which the importer keeps as the canvas");
    assert_eq!(
        imported.objects.len(),
        LETTERS.len(),
        "letters.svg must carry one group per letter of the wordmark"
    );

    // The em frame onto the hero's wordmark: same type size, same baseline,
    // centered in the stage the way the hero centers its own word.
    let scale = WORD_PX / LETTER_EM;
    let left = STAGE.0 / 2.0 - svg_width * scale / 2.0;
    let baseline = WORD_BASELINE_Y - LETTER_BASELINE * scale;

    imported
        .objects
        .into_iter()
        .zip(LETTERS)
        .map(|(object, (name, gaps))| {
            let Map2dShape::Path(path) = object.shape else {
                unreachable!("the SVG importer only ever emits path objects")
            };
            let points: Vec<[f32; 2]> = path
                .points
                .iter()
                .map(|[x, y]| round_point([left + x * scale, baseline + y * scale]))
                .collect();
            assert!(
                gaps.iter().all(|gap| (*gap as usize) < points.len() - 1),
                "{name}: jumper segment index out of range"
            );
            Map2dObject {
                name: name.to_string(),
                shape: Map2dShape::Path(PathShape {
                    points,
                    gaps: gaps.to_vec(),
                    ..path
                }),
                ..object
            }
        })
        .collect()
}

/// Two decimals, so the committed document reads like a drawing rather than
/// like float noise. Hundredths of a doc unit are far below the lattice.
fn round_point(point: [f32; 2]) -> [f32; 2] {
    [
        (point[0] * 100.0).round() / 100.0,
        (point[1] * 100.0).round() / 100.0,
    ]
}

use std::path::PathBuf;

fn map2d_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/logo-sign/sign.map2d.json")
}

/// Drift gate: the committed mapping document must match the generated one,
/// so a brand-geometry change cannot silently leave the artwork the hero's
/// pencil opens behind.
#[test]
fn logo_sign_map2d_in_sync() {
    let on_disk =
        std::fs::read_to_string(map2d_path()).expect("read examples/logo-sign/sign.map2d.json");
    assert_eq!(
        on_disk,
        logo_sign_map2d_json(),
        "examples/logo-sign/sign.map2d.json is stale. Regenerate:\n  \
         cargo test -p lpa-studio-web logo_sign_map2d_regen -- --ignored"
    );
}

/// Regenerator (opt-in): rewrites the committed document from the brand
/// geometry and the letter SVG. Run after changing either.
#[test]
#[ignore = "writes examples/logo-sign/sign.map2d.json; run explicitly after geometry changes"]
fn logo_sign_map2d_regen() {
    std::fs::write(map2d_path(), logo_sign_map2d_json())
        .expect("write examples/logo-sign/sign.map2d.json");
}

/// The counts the artwork IS: a 132-lamp shaped matrix and 109 lamps of
/// wordmark, which is also how `output.json` splits its two connectors. A
/// pitch nudge that quietly resized the board would land as a wrong port
/// count out in the example; here it lands as a failing assert.
#[test]
fn logo_sign_lamp_counts_are_pinned() {
    let doc = logo_sign_map2d();
    let resolved = lpc_mapping::resolve(&doc).expect("the logo sign resolves");
    let matrix = resolved.object_span(0).expect("the matrix resolves").count;
    assert_eq!(matrix, 132, "shaped matrix lamps");
    assert_eq!(
        resolved.lamps.len() as u32 - matrix,
        109,
        "letter string lamps"
    );
    assert_eq!(resolved.lamps.len(), 241, "the whole sign");
    // One span per object: the three jumpered letters stay one strand each in
    // wiring order (their breaks are `path_gap_breaks`, not span boundaries).
    assert_eq!(resolved.spans.len(), 12);
}

/// The document declares format 5 — the `filled_polygon` floor — and frames
/// itself on the hero's stage.
#[test]
fn logo_sign_map2d_is_a_format_5_stage() {
    let doc = logo_sign_map2d();
    assert_eq!(doc.format, 5);
    assert_eq!(doc.canvas, Some([0.0, 0.0, 300.0, 308.0]));
    assert_eq!(doc.objects.len(), 12);
    assert_eq!(doc.objects[0].name, "matrix");
    let names: Vec<&str> = doc.objects[1..]
        .iter()
        .map(|object| object.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["L", "i", "g", "h", "t", "P", "l", "a", "y", "e", "r"]
    );
}

/// The matrix outline IS the hero's clip path: every tangent point the `d`
/// string is built from must appear, to the same two decimals, in the
/// flattened outline. Faceting the arcs may add points; moving the triangle
/// may not.
#[test]
fn logo_sign_matrix_outline_traces_the_hero_triangle() {
    let (cx, cy, r) = stage_triangle();
    let rho = r * HERO_CORNER_RATIO;
    let doc = logo_sign_map2d();
    let Map2dShape::FilledPolygon(matrix) = &doc.objects[0].shape else {
        panic!("object 1 is the shaped matrix")
    };
    for [enter, leave] in fillet_tri_corners(cx, cy, r, rho) {
        for tangent in [enter, leave] {
            let point = round_point([tangent.0, tangent.1]);
            assert!(
                matrix.points.contains(&point),
                "tangent point {point:?} is missing from the matrix outline"
            );
        }
    }
}
