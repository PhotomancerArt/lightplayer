//! Statements: how arrows lay out, one sentence per test.
//!
//! Each test's doc comment IS the statement; read this file as the list.
//! When a statement changes, change the sentence first, then the code.
//!
//! Text is measured with `AverageCharWidth` (0.6 em) and the default
//! `LayoutOptions` (10-unit primary text, 9-unit quiet, 4-unit clearance).

use lab_arrows::*;

// ---- labels lie along their arrow ------------------------------------------

/// A label lies along its arrow: its box's angle matches the curve's
/// direction where it sits.
#[test]
fn a_label_lies_along_its_arrow() {
    let a = gentle_s().label(Label::primary("tail → GPIO 10"));
    let g = layout(&a);
    let p = placed(&g, 0);
    let line = Path::of_cubic(&a.centreline(), false).polyline();
    let tangent = line.tangent_at(p.along * line.length());
    let expected = upright(tangent.angle_deg());
    assert!(
        (p.angle_deg - expected).abs() < 5.0,
        "box at {}°, curve at {expected}°",
        p.angle_deg
    );
    assert!(
        line.distance_to(p.frame.centre) < 0.5,
        "and its centre is on the line"
    );
    assert_eq!(
        p.angle_deg, p.frame.angle_deg,
        "the text turns with its box"
    );
}

/// A label never reads upside down: whichever way its arrow runs, the text's
/// angle is in (−90°, 90°].
#[test]
fn a_label_never_reads_upside_down() {
    for i in 0..24 {
        let dir = Vec2::from_angle_deg(i as f64 * 15.0 + 1.0);
        for a in [
            Arrow::new(Vec2::ZERO, dir * 200.0).straight(),
            Arrow::new(Vec2::ZERO, dir * 200.0).arrive(EndDirection::Side(Side::Left)),
        ] {
            let g = layout(&a.label(Label::primary("which way up")));
            let p = placed(&g, 0);
            assert!(
                p.angle_deg > -90.0 && p.angle_deg <= 90.0,
                "an arrow at {}° set its text at {}°",
                i * 15 + 1,
                p.angle_deg
            );
        }
    }
}

/// A label that cannot sit where it prefers slides along its line to the
/// nearest straight stretch, and says it moved.
#[test]
fn a_label_on_a_bend_slides_to_a_straight_stretch_and_says_so() {
    let a = Arrow::new(Vec2::new(80.0, 200.0), Vec2::new(200.0, 70.0))
        .route(Route::Curve(Bend {
            reach: 0.4,
            min: 110.0,
        }))
        .leave(EndDirection::Side(Side::Top))
        .arrive(EndDirection::Side(Side::Left))
        .label(Label::primary("ring display"));
    let g = layout(&a);
    assert_eq!(g.labels[0].fit, LabelFit::Along { moved: true });
    assert!((placed(&g, 0).along - 0.5).abs() > 0.05);
}

/// A label at its preferred spot says it did not move.
#[test]
fn a_label_where_it_prefers_did_not_move() {
    let g = layout(&straight(300.0).label(Label::primary("lamps 0–49")));
    assert_eq!(g.labels[0].fit, LabelFit::Along { moved: false });
    assert!((placed(&g, 0).along - 0.5).abs() < 1e-9);
}

/// The text is anchored at its box's centre, in the middle both ways.
#[test]
fn the_text_sits_at_the_centre_of_its_box() {
    let g = layout(&gentle_s().label(Label::quiet("control.out")));
    let p = placed(&g, 0);
    assert_eq!(p.text_at, p.frame.centre);
    assert_eq!(p.anchor, TextAnchor::Middle);
    assert_eq!(p.baseline, TextBaseline::Central);
}

/// A label's box is the text plus its padding: the measured width plus the
/// x padding each side, the font size plus the y padding each side.
#[test]
fn a_labels_box_is_its_text_plus_padding() {
    let g = layout(&straight(300.0).label(Label::primary("abcde")));
    let f = placed(&g, 0).frame;
    assert!((f.width - (5.0 * 6.0 + 8.0)).abs() < 1e-9);
    assert!((f.height - (10.0 + 4.0)).abs() < 1e-9);
}

// ---- clear of the heads ------------------------------------------------------

/// A label stays clear of the arrowhead and the endpoints, by at least the
/// clearance, even when it prefers to sit right at an end.
#[test]
fn a_label_stays_clear_of_the_heads_and_the_ends() {
    let clearance = LayoutOptions::default().clearance;
    for placement in [Placement::NearStart, Placement::Centre, Placement::NearEnd] {
        for a in [straight(160.0), gentle_s()] {
            let a = a
                .start_head(Some(Head::default()))
                .label(Label::primary("near an end").at(placement));
            let g = layout(&a);
            let grown = placed(&g, 0).frame.inflated(clearance - 1e-6);
            assert!(!grown.contains(a.from) && !grown.contains(a.to));
            for h in &g.heads {
                assert!(!grown.overlaps_polygon(&h.points), "{placement:?}");
            }
        }
    }
}

/// The line stops at the arrowhead instead of running through it: it ends
/// on a triangle's base, a head's length short of the tip.
#[test]
fn the_line_stops_at_the_arrowhead() {
    for a in [straight(200.0), gentle_s()] {
        let a = a.start_head(Some(Head::triangle(8.0, 6.0)));
        let g = layout(&a);
        let points = g.path.polyline().points().to_vec();
        let (first, last) = (points[0], *points.last().unwrap());
        assert!(
            (last.distance(a.to) - 7.0).abs() < 1e-3,
            "end head is 7 long"
        );
        assert!(
            (first.distance(a.from) - 8.0).abs() < 1e-3,
            "start head is 8 long"
        );
        for h in &g.heads {
            let base_mid = h.points[1].lerp(h.points[2], 0.5);
            let end = if h.end == ArrowEnd::End { last } else { first };
            assert!(
                end.distance(base_mid) < 0.05,
                "the line meets the base's middle"
            );
        }
    }
}

/// A chevron is open, so the line runs right to its tip.
#[test]
fn a_chevrons_line_runs_to_its_tip() {
    let a = straight(200.0).end_head(Some(Head::chevron(8.0, 9.0)));
    let g = layout(&a);
    let last = *g.path.polyline().points().last().unwrap();
    assert!(last.distance(a.to) < 1e-9);
    assert!(!g.heads[0].closed);
    assert_eq!(
        g.heads[0].points[1], a.to,
        "the chevron's middle point is the tip"
    );
}

/// A head points the way the line arrives: arriving from the left, it points
/// right, onto its target.
#[test]
fn a_head_points_the_way_the_line_arrives() {
    let a = Arrow::new(Vec2::new(0.0, 0.0), Vec2::new(200.0, 80.0))
        .arrive(EndDirection::Side(Side::Left));
    let g = layout(&a);
    let h = &g.heads[0];
    let base_mid = h.points[1].lerp(h.points[2], 0.5);
    let dir = (h.tip - base_mid).normalized();
    assert!(dir.x > 0.99, "points right, got {dir:?}");
}

// ---- moved, beside, dropped ----------------------------------------------------

/// A label too long for its arrow goes level beside the arrow's middle,
/// and says so.
#[test]
fn a_label_too_long_for_its_arrow_goes_beside_it() {
    let a = straight(30.0).label(Label::primary("a long label"));
    let g = layout(&a);
    assert_eq!(g.labels[0].fit, LabelFit::Beside);
    let p = placed(&g, 0);
    assert_eq!(p.angle_deg, 0.0, "level");
    let line = g.path.polyline();
    assert!(
        line.points()
            .windows(2)
            .all(|w| !p.frame.overlaps_polygon(&[w[0], w[1]])),
        "beside the line, not over it"
    );
}

/// A label with nowhere to go is dropped, says why, and has no box.
#[test]
fn a_label_with_nowhere_to_go_is_dropped_and_says_why() {
    let tiny = layout(&straight(8.0).label(Label::primary("tiny")));
    assert_eq!(tiny.labels[0].fit, LabelFit::Dropped(DropReason::TooShort));
    assert!(tiny.labels[0].placed.is_none());

    let no_beside = LayoutOptions {
        beside: false,
        ..LayoutOptions::default()
    };
    let short = straight(30.0).label(Label::primary("a long label"));
    let g = short.layout(&AverageCharWidth::default(), &no_beside);
    assert_eq!(g.labels[0].fit, LabelFit::Dropped(DropReason::TooShort));
}

// ---- several arrows -------------------------------------------------------------

/// Labels on neighbouring arrows don't overlap: arrows converging on
/// neighbouring targets each get their own spot, or drop as crowded.
#[test]
fn labels_on_neighbouring_arrows_do_not_overlap() {
    let arrows: Vec<Arrow> = (0..8)
        .map(|i| {
            Arrow::new(
                Vec2::new(40.0 + (i % 2) as f64 * 30.0, 40.0 + i as f64 * 24.0),
                Vec2::new(330.0, 90.0 + i as f64 * 11.0),
            )
            .arrive(EndDirection::Side(Side::Left))
            .label(Label::primary(format!("lamps {}–{}", i * 10, i * 10 + 9)))
            .label(Label::quiet("control.out").near_end())
        })
        .collect();
    let g = layout_arrows(
        &arrows,
        &AverageCharWidth::default(),
        &LayoutOptions::default(),
    );
    let boxes: Vec<OrientedBox> = g
        .iter()
        .flat_map(|a| &a.labels)
        .filter_map(|l| l.placed.map(|p| p.frame))
        .collect();
    assert!(boxes.len() >= 8, "most of them find a spot");
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            assert!(!a.overlaps(b), "{a:?} overlaps {b:?}");
        }
    }
    for l in g.iter().flat_map(|a| &a.labels) {
        assert_eq!(l.placed.is_some(), l.fit.is_placed());
    }
}

/// Labels on one arrow don't overlap each other either.
#[test]
fn labels_on_one_arrow_do_not_overlap() {
    let a = straight(200.0)
        .label(Label::primary("first"))
        .label(Label::primary("second"))
        .label(Label::quiet("third"));
    let g = layout(&a);
    let boxes: Vec<OrientedBox> = g
        .labels
        .iter()
        .filter_map(|l| l.placed.map(|p| p.frame))
        .collect();
    assert_eq!(boxes.len(), 3);
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            assert!(!a.overlaps(b));
        }
    }
}

/// A quiet label gives way to a primary one, whichever comes first: the
/// primary keeps the spot it prefers.
#[test]
fn a_quiet_label_gives_way_to_a_primary_one() {
    let quiet_first = straight(200.0).label(Label::quiet("channel"));
    let primary = Arrow::new(Vec2::new(0.0, 12.0), Vec2::new(200.0, 12.0))
        .straight()
        .label(Label::primary("span"));
    let g = layout_arrows(
        &[quiet_first, primary],
        &AverageCharWidth::default(),
        &LayoutOptions::default(),
    );
    assert_eq!(g[1].labels[0].fit, LabelFit::Along { moved: false });
    assert_ne!(g[0].labels[0].fit, LabelFit::Along { moved: false });
}

/// The same input always gives the same geometry.
#[test]
fn the_same_input_always_gives_the_same_geometry() {
    let arrows = vec![
        gentle_s()
            .label(Label::primary("tail → GPIO 10"))
            .label(Label::quiet("control.out").near_end()),
        straight(300.0).label(Label::primary("lamps 0–49")),
        straight(30.0).label(Label::primary("a long label")),
    ];
    let m = AverageCharWidth::default();
    let o = LayoutOptions::default();
    assert_eq!(
        layout_arrows(&arrows, &m, &o),
        layout_arrows(&arrows, &m, &o)
    );
}

// ---- the line --------------------------------------------------------------------

/// A smooth arrow leaves the way it is told and arrives the way it is told:
/// heading toward its target, and in from the left.
#[test]
fn a_smooth_arrow_leaves_and_arrives_as_told() {
    let a = Arrow::new(Vec2::new(0.0, 0.0), Vec2::new(200.0, 100.0))
        .leave(EndDirection::Direct)
        .arrive(EndDirection::Side(Side::Left));
    let c = a.centreline();
    let out = (c.c1 - c.start).normalized();
    let toward = (a.to - a.from).normalized();
    assert!(out.distance(toward) < 1e-9);
    let inward = (c.end - c.c2).normalized();
    assert!(inward.distance(Vec2::new(1.0, 0.0)) < 1e-9);
}

/// The line comes out as segments and as an SVG path.
#[test]
fn the_line_comes_out_as_segments_and_svg() {
    let g = layout(&straight(100.0).end_head(None));
    assert_eq!(
        g.path.segments,
        vec![
            Segment::MoveTo(Vec2::ZERO),
            Segment::LineTo(Vec2::new(100.0, 0.0))
        ]
    );
    assert_eq!(g.path.to_svg(), "M0,0 L100,0");
    let curved = layout(&gentle_s().end_head(None));
    assert!(matches!(curved.path.segments[1], Segment::CubicTo { .. }));
    assert!(curved.path.to_svg().starts_with("M40,70 C"));
}

/// Units are the caller's: the same arrow ten times bigger, with ten times
/// the sizes, lays out ten times bigger.
#[test]
fn units_are_the_callers() {
    let small = gentle_s().label(Label::primary("tail → GPIO 10"));
    let o = LayoutOptions::default();
    let big = Arrow {
        from: small.from * 10.0,
        to: small.to * 10.0,
        route: Route::Curve(Bend {
            reach: 0.4,
            min: 200.0,
        }),
        end_head: Some(Head::triangle(70.0, 70.0)),
        ..small.clone()
    };
    let big_options = LayoutOptions {
        font_size_primary: o.font_size_primary * 10.0,
        font_size_quiet: o.font_size_quiet * 10.0,
        padding: o.padding * 10.0,
        clearance: o.clearance * 10.0,
        gap: o.gap * 10.0,
        ..o
    };
    let (a, b) = (
        placed(&layout(&small), 0),
        placed(&big.layout(&AverageCharWidth::default(), &big_options), 0),
    );
    assert!(b.frame.centre.distance(a.frame.centre * 10.0) < 1e-6);
    assert!((b.frame.width - a.frame.width * 10.0).abs() < 1e-6);
    assert!((b.angle_deg - a.angle_deg).abs() < 1e-9);
}

/// An arrow from a point to itself draws no line and drops its labels,
/// without any NaN.
#[test]
fn an_arrow_to_its_own_start_draws_nothing() {
    let a = Arrow::new(Vec2::new(5.0, 5.0), Vec2::new(5.0, 5.0))
        .straight()
        .label(Label::primary("nowhere"));
    let g = layout(&a);
    assert!(matches!(g.labels[0].fit, LabelFit::Dropped(_)));
    for h in &g.heads {
        assert!(h.points.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
    }
}

// ---- the gutter route --------------------------------------------------------------

/// A gutter edge leaves its start sideways, runs straight along its lane,
/// and enters its end sideways, with rounded corners between: its line is
/// level at both ends and upright on the lane.
#[test]
fn a_gutter_edge_runs_out_along_its_lane_and_back_in() {
    let a = gutter(Vec2::new(100.0, 40.0), Vec2::new(100.0, 260.0), 20.0);
    let line = a.line().polyline();
    let pts = line.points();
    assert_eq!(pts.first(), Some(&a.from));
    assert_eq!(pts.last(), Some(&a.to));
    assert!(line.tangent_at(1.0).y.abs() < 1e-9, "leaves level");
    assert!(
        line.tangent_at(line.length() - 1.0).y.abs() < 1e-9,
        "arrives level"
    );
    let mid = line.point_at(line.length() / 2.0);
    assert!((mid.x - 20.0).abs() < 1e-9, "on the lane: {mid:?}");
    assert!(
        line.tangent_at(line.length() / 2.0).x.abs() < 1e-9,
        "upright there"
    );
    assert!(
        pts.iter()
            .all(|p| p.x >= 20.0 - 1e-9 && p.x <= 100.0 + 1e-9),
        "never past its lane"
    );
}

/// A gutter edge's head points into its end, across from the lane, and
/// the line stops at the head's base.
#[test]
fn a_gutter_edges_head_points_into_its_end() {
    let a = gutter(Vec2::new(100.0, 40.0), Vec2::new(100.0, 260.0), 20.0);
    let g = layout(&a);
    let head = &g.heads[0];
    assert_eq!(head.tip, a.to);
    let line = g.path.polyline();
    let end = *line.points().last().unwrap();
    assert!(
        end.x < a.to.x && (end.y - a.to.y).abs() < 1e-9,
        "the line stops short: {end:?}"
    );
}

/// A gutter edge's words lie on the lane, turned to run along it, upright.
#[test]
fn a_gutter_edges_label_lies_on_its_lane() {
    let a = gutter(Vec2::new(100.0, 40.0), Vec2::new(100.0, 360.0), 20.0)
        .label(Label::quiet("visual.out"));
    let p = placed(&layout(&a), 0);
    assert!(
        (p.frame.centre.x - 20.0).abs() < 1e-6,
        "{:?}",
        p.frame.centre
    );
    assert!((p.angle_deg.abs() - 90.0).abs() < 1e-6, "{}", p.angle_deg);
    assert!(p.angle_deg > -90.0 && p.angle_deg <= 90.0);
}

/// Where the legs are too short for the corners, the corners are smaller;
/// level ends make a straight line along the lane and back.
#[test]
fn a_gutter_edges_corners_shrink_to_fit() {
    let a = gutter(Vec2::new(30.0, 40.0), Vec2::new(30.0, 50.0), 20.0);
    let line = a.line().polyline();
    assert!(line.points().iter().all(|p| p.x >= 20.0 - 1e-9));
    assert_eq!(line.points().last(), Some(&a.to));
}

// ---- helpers ---------------------------------------------------------------------

fn layout(a: &Arrow) -> ArrowGeometry {
    a.layout(&AverageCharWidth::default(), &LayoutOptions::default())
}

fn placed(g: &ArrowGeometry, i: usize) -> PlacedLabel {
    g.labels[i]
        .placed
        .unwrap_or_else(|| panic!("label {i} was not placed: {:?}", g.labels[i].fit))
}

/// A straight arrow along +x from the origin.
fn straight(length: f64) -> Arrow {
    Arrow::new(Vec2::ZERO, Vec2::new(length, 0.0)).straight()
}

/// The gallery's gentle S: leaves right, arrives from the left.
fn gentle_s() -> Arrow {
    Arrow::new(Vec2::new(40.0, 70.0), Vec2::new(340.0, 180.0))
        .leave(EndDirection::Side(Side::Right))
        .arrive(EndDirection::Side(Side::Left))
}

fn upright(deg: f64) -> f64 {
    let mut d = deg;
    while d > 90.0 {
        d -= 180.0;
    }
    while d <= -90.0 {
        d += 180.0;
    }
    d
}

/// The stack's edge: out of a card's left side at `from`, along the lane
/// at `lane_x`, into the target's left side at `to`.
fn gutter(from: Vec2, to: Vec2, lane_x: f64) -> Arrow {
    Arrow::new(from, to).route(Route::Gutter(Gutter {
        lane_x,
        radius: 8.0,
    }))
}
