//! Statements: fixture and shader space (`lpc_mapping::space`), one test
//! per sentence. The theory is planning
//! `lp2025/2026-10-28-0935-fixture-space-theory` (`theory.md` D3–D28).
//!
//! ```bash
//! cargo test -p lpc-mapping --test space_statements
//! ```

use lpc_mapping::space::{
    AddressMode, AlignAxis, DEFAULT_VIEW_N, Fit, FitAlign, FitMode, OutsideRule, ProductBox,
    Q16_ONE, REL_ONE, Sample, SpaceError, SpaceTransform, UnitScope, ViewBox, order_position_q16,
};

/// Cover of the unit view on a 3:1 box gives a `unit` extent of (1, 1/3).
#[test]
fn cover_of_the_unit_view_on_a_three_to_one_box_gives_an_extent_of_one_by_a_third() {
    let t = unit(view_box(0.0, 0.0, 300.0, 100.0), Fit::DEFAULT);
    let scope = UnitScope::new(&t, 10.0, 31);
    assert_eq!(scope.extent[0], 1.0);
    assert_eq!(scope.extent[1], (1.0f64 / 3.0) as f32);
    assert_eq!(scope.lamp_count, 31);
    assert_eq!(
        scope.pitch,
        (10.0f64 / 150.0) as f32,
        "a pitch of 10 over a half-width of 150"
    );
}

/// Contain of the unit view on a 3:1 box gives an extent of (3, 1).
#[test]
fn contain_of_the_unit_view_on_a_three_to_one_box_gives_an_extent_of_three_by_one() {
    let t = unit(
        view_box(0.0, 0.0, 300.0, 100.0),
        Fit::new(FitMode::Contain, FitAlign::CENTRE),
    );
    assert_eq!(UnitScope::new(&t, 1.0, 1).extent, [3.0, 1.0]);
}

/// Fill maps the box's corners to the view's corners.
#[test]
fn fill_maps_the_boxs_corners_to_the_views_corners() {
    let t = unit(
        view_box(10.0, 20.0, 300.0, 100.0),
        Fit::new(FitMode::Fill, FitAlign::CENTRE),
    );
    assert_eq!(at(&t, [10.0, 20.0]), q16s(-1.0, 1.0), "top-left");
    assert_eq!(at(&t, [310.0, 20.0]), q16s(1.0, 1.0), "top-right");
    assert_eq!(at(&t, [10.0, 120.0]), q16s(-1.0, -1.0), "bottom-left");
    assert_eq!(at(&t, [310.0, 120.0]), q16s(1.0, -1.0), "bottom-right");
}

/// Cover, centred, of the unit view on the lamps' own bounds gives the same
/// Q16 coordinates as today's pattern frame, bit for bit: the cases from
/// `lpc-engine`'s pattern-space tests, and the PLAYFUL choker's lamps
/// through today's whole pipeline (y extent ±0.1377).
#[test]
fn cover_of_the_unit_view_on_the_lamps_bounds_is_todays_pattern_frame_bit_for_bit() {
    // A 16 × 16 grid of texel centres.
    let grid: Vec<[i32; 2]> = (0..16)
        .flat_map(|y| (0..16).map(move |x| [texel(x), texel(y)]))
        .collect();
    // A box 40 × 10 pixels wide, with lamps on its corners, edges and inside.
    let wide: Vec<[i32; 2]> = [
        [10.0, 20.0],
        [50.0, 30.0],
        [30.0, 25.0],
        [30.0, 20.0],
        [12.25, 29.5],
        [49.0, 21.125],
    ]
    .iter()
    .map(|&[x, y]| [q16(x), q16(y)])
    .collect();
    // A tall box: the long side is y.
    let tall: Vec<[i32; 2]> = (0..9)
        .map(|k| [q16(3.0 + (k % 3) as f32), q16(1.5 + 7.25 * k as f32)])
        .collect();
    // A one-row strip of 100 lamps: zero height. The box pads that axis
    // (to one pitch); cover is set by the long side, so T does not move.
    let strip: Vec<[i32; 2]> = (0..100).map(|k| [texel(k), q16(3.0)]).collect();

    for (name, lamps, pad) in [
        ("16x16 grid", grid, 0),
        ("40x10 box", wide, 0),
        ("tall box", tall, 0),
        ("100-lamp strip", strip, Q16_ONE),
    ] {
        let today = TodayFrame::new(&lamps);
        let t = lamp_box_cover(&lamps, pad);
        for lamp in &lamps {
            assert_eq!(
                at(&t, [lamp[0] as f32, lamp[1] as f32]),
                today.map(*lamp),
                "{name}: lamp {lamp:?}"
            );
        }
        if pad == 0 {
            assert_eq!(t.box_half_extent(), today.extent(), "{name}: extent");
        }
    }

    // The choker: its map2d, fitted into its 104 × 28 render the way the
    // engine loads it, turned into the pixel coordinates the engine streams.
    let lamps = choker_pixel_lamps();
    let today = TodayFrame::new(&lamps);
    assert!(
        (today.extent()[1] - 0.1377).abs() < 5e-5,
        "the choker's y extent is ±0.1377: {:?}",
        today.extent()
    );
    let t = lamp_box_cover(&lamps, 0);
    assert_eq!(t.box_half_extent(), today.extent());
    for lamp in &lamps {
        assert_eq!(
            at(&t, [lamp[0] as f32, lamp[1] as f32]),
            today.map(*lamp),
            "choker lamp {lamp:?}"
        );
    }
}

/// The fixture's top edge is the source's top: y flips inside T.
#[test]
fn the_fixtures_top_edge_is_the_sources_top() {
    let t = unit(view_box(0.0, 0.0, 100.0, 100.0), Fit::DEFAULT);
    assert_eq!(at(&t, [50.0, 0.0]), q16s(0.0, 1.0), "top middle is y = +1");
    assert_eq!(
        at(&t, [50.0, 100.0]),
        q16s(0.0, -1.0),
        "bottom middle is y = −1"
    );
    let toy = shadertoy(view_box(0.0, 0.0, 100.0, 100.0), Fit::DEFAULT, 64);
    assert_eq!(
        at(&toy, [0.0, 0.0]),
        q16s(0.0, 64.0),
        "fragCoord's origin is bottom-left"
    );
    assert_eq!(at(&toy, [0.0, 100.0]), q16s(0.0, 0.0));
}

/// Each of the nine alignments puts the product where SVG's
/// `preserveAspectRatio` would, under contain (meet) and under cover
/// (slice).
#[test]
fn each_alignment_places_the_product_where_svgs_preserve_aspect_ratio_would() {
    use AlignAxis::{Max, Mid, Min};
    // Contain on a 3:1 box (300 × 100): the square view is 100 × 100, with
    // 200 units of room along x. The box's left edge sees the view's x at
    // −1 (xMin), −3 (xMid), −5 (xMax); along y there is no room.
    // Cover on the same box: the view is 300 × 300, cut, with 200 units of
    // overflow along y. The box's top edge sees y at +1
    // (yMin), +1/3 (yMid), −1/3 (yMax); along x there is no overflow.
    let wide = view_box(0.0, 0.0, 300.0, 100.0);
    for x in [Min, Mid, Max] {
        for y in [Min, Mid, Max] {
            let align = FitAlign::new(x, y);
            let contain = unit(wide, Fit::new(FitMode::Contain, align));
            let left_x = match x {
                Min => -1.0,
                Mid => -3.0,
                Max => -5.0,
            };
            assert_eq!(
                at(&contain, [0.0, 0.0]),
                q16s(left_x, 1.0),
                "contain {x:?} {y:?}: the box's top-left corner"
            );
            let cover = unit(wide, Fit::new(FitMode::Cover, align));
            let top_y = match y {
                Min => 1.0,
                Mid => 1.0 / 3.0,
                Max => -1.0 / 3.0,
            };
            assert_eq!(
                at(&cover, [0.0, 0.0]),
                q16s(-1.0, top_y),
                "cover {x:?} {y:?}: the box's top-left corner"
            );
        }
    }
}

/// Scale 2 zooms in about the fitted centre; rotation 90° (clockwise as the
/// fixture is drawn) about the centre maps the box's right edge to the
/// source's top.
#[test]
fn scale_zooms_in_about_the_centre_and_a_quarter_turn_puts_the_top_on_the_right() {
    let square = view_box(0.0, 0.0, 100.0, 100.0);
    let zoomed = unit(square, Fit::DEFAULT.with_scale(2.0));
    assert_eq!(
        at(&zoomed, [100.0, 50.0]),
        q16s(0.5, 0.0),
        "the right edge sees x = 0.5"
    );
    assert_eq!(
        at(&zoomed, [50.0, 50.0]),
        q16s(0.0, 0.0),
        "the centre stays"
    );

    let turned = unit(square, Fit::DEFAULT.with_rotation_deg(90.0));
    assert_eq!(
        at(&turned, [100.0, 50.0]),
        q16s(0.0, 1.0),
        "right edge → the source's top"
    );
    assert_eq!(
        at(&turned, [50.0, 0.0]),
        q16s(-1.0, 0.0),
        "top edge → the source's left"
    );
    assert_eq!(
        at(&turned, [50.0, 50.0]),
        q16s(0.0, 0.0),
        "the centre stays"
    );
}

/// A lamp on the box's edge is inside; a lamp just past it is dark.
#[test]
fn a_lamp_on_the_edge_is_inside_and_one_just_past_it_is_dark() {
    let t = unit(view_box(0.0, 0.0, 100.0, 50.0), Fit::DEFAULT);
    assert!(
        matches!(t.apply([100.0, 50.0]), Sample::At(_)),
        "the far corner"
    );
    assert!(
        matches!(t.apply([0.0, 0.0]), Sample::At(_)),
        "the near corner"
    );
    let past = f32::from_bits(100.0f32.to_bits() + 1);
    assert_eq!(t.apply([past, 25.0]), Sample::Dark);
    assert_eq!(t.apply([50.0, -0.001]), Sample::Dark);
    assert_eq!(t.apply_rel([REL_ONE, REL_ONE]), t.apply([100.0, 50.0]));
    assert_eq!(t.apply_rel([REL_ONE + 1, 0]), Sample::Dark);
    assert_eq!(t.apply_rel([0, -1]), Sample::Dark);
}

/// A sample past a bounded extent is outside, and each address mode
/// resolves it as the GPU would (wrap, mirror, clamp, solid → not
/// evaluated), per axis.
#[test]
fn a_sample_past_an_extent_is_outside_and_each_address_mode_resolves_it_as_a_gpu_would() {
    // Contain of a 10 × 10 extent, aligned left, on a 30 × 10 box: the box's
    // x runs 0…30 in extent units, so x past 10 is outside.
    let t = SpaceTransform::new(
        view_box(0.0, 0.0, 30.0, 10.0),
        Fit::new(
            FitMode::Contain,
            FitAlign::new(AlignAxis::Min, AlignAxis::Mid),
        ),
        ProductBox::extent(10.0, 10.0).unwrap(),
    )
    .unwrap();
    assert_eq!(t.apply([5.0, 5.0]), Sample::At(q16s(5.0, 5.0)));
    assert_eq!(
        t.apply([10.0, 5.0]),
        Sample::At(q16s(10.0, 5.0)),
        "the edge is inside"
    );
    let past = t.apply([13.0, 2.0]);
    assert_eq!(past, Sample::Outside(q16s(13.0, 2.0)));
    let far = t.apply([27.0, 2.0]);

    let rule = |x| OutsideRule {
        x,
        y: AddressMode::Solid,
        solid: [255, 0, 0],
    };
    assert_eq!(
        t.resolve(past, &rule(AddressMode::Clamp)),
        Some(q16s(10.0, 2.0))
    );
    assert_eq!(
        t.resolve(past, &rule(AddressMode::Wrap)),
        Some(q16s(3.0, 2.0))
    );
    assert_eq!(
        t.resolve(past, &rule(AddressMode::Mirror)),
        Some(q16s(7.0, 2.0))
    );
    assert_eq!(
        t.resolve(far, &rule(AddressMode::Wrap)),
        Some(q16s(7.0, 2.0))
    );
    assert_eq!(
        t.resolve(far, &rule(AddressMode::Mirror)),
        Some(q16s(7.0, 2.0))
    );
    assert_eq!(
        t.resolve(past, &rule(AddressMode::Solid)),
        None,
        "solid: not evaluated"
    );
    // y is inside, so its Solid mode never fires.
    assert_eq!(
        t.resolve(t.apply([5.0, 5.0]), &rule(AddressMode::Solid)),
        Some(q16s(5.0, 5.0))
    );
    assert_eq!(t.resolve(Sample::Dark, &rule(AddressMode::Clamp)), None);
}

/// On a 30,000-lamp strip, the last lamp's order position is exactly 1.0 and
/// its `unit` x under cover is exactly 1.0.
#[test]
fn a_thirty_thousand_lamp_strips_last_lamp_is_exactly_one_along_the_order_and_in_unit_x() {
    let count = 30_000u32;
    assert_eq!(order_position_q16(0, count), 0);
    assert_eq!(order_position_q16(count - 1, count), Q16_ONE);
    for k in [1u32, 1000, 14_999, 29_998] {
        let want = (f64::from(k) / f64::from(count - 1) * 65536.0).round() as i32;
        assert_eq!(order_position_q16(k, count), want, "lamp {k}");
    }
    assert_eq!(order_position_q16(0, 1), 0, "one lamp sits at 0");

    // The strip's lamps one unit apart; the box pads y to one pitch.
    let t = unit(view_box(0.0, -0.5, (count - 1) as f32, 1.0), Fit::DEFAULT);
    assert_eq!(at(&t, [0.0, 0.0])[0], -Q16_ONE);
    assert_eq!(at(&t, [(count - 1) as f32, 0.0])[0], Q16_ONE);
    let mid = at(&t, [14_999.5, 0.0]);
    assert_eq!(mid, [0, 0]);
}

/// A 40 m × 2 m box in millimetres does not overflow and keeps a 30 k
/// strip's lamps distinct.
#[test]
fn a_forty_metre_box_in_millimetres_keeps_a_thirty_thousand_lamp_strip_distinct() {
    let count = 30_000u32;
    let t = unit(view_box(0.0, 0.0, 40_000.0, 2_000.0), Fit::DEFAULT);
    let pitch = 40_000.0 / (count - 1) as f32;
    let xs: Vec<i32> = (0..count)
        .map(|k| at(&t, [(k as f32 * pitch).min(40_000.0), 1_000.0])[0])
        .collect();
    assert_eq!(xs[0], -Q16_ONE);
    assert_eq!(*xs.last().unwrap(), Q16_ONE);
    assert!(
        xs.windows(2).all(|w| w[0] < w[1]),
        "every lamp distinct, in order"
    );
    assert_eq!(t.box_half_extent(), [1.0, 0.05]);
    // And in shadertoy pixels, and under contain (larger numbers).
    let toy = shadertoy(view_box(0.0, 0.0, 40_000.0, 2_000.0), Fit::DEFAULT, 64);
    assert_eq!(at(&toy, [40_000.0, 1_000.0]), q16s(64.0, 32.0));
    let contain = unit(
        view_box(0.0, 0.0, 40_000.0, 2_000.0),
        Fit::new(FitMode::Contain, FitAlign::CENTRE),
    );
    assert_eq!(at(&contain, [40_000.0, 0.0]), q16s(20.0, 1.0));
}

/// Changing the source (unit → shadertoy(64) → extent(32, 16)) rebuilds only
/// T: the same lamp list resolves under each.
#[test]
fn a_source_change_rebuilds_only_t_and_the_same_lamps_resolve_under_each() {
    let box_ = view_box(0.0, 0.0, 200.0, 100.0);
    let lamps: Vec<[f32; 2]> = (0..=10)
        .flat_map(|i| (0..=5).map(move |j| [i as f32 * 20.0, j as f32 * 20.0]))
        .collect();
    let sources = [
        ProductBox::unit_2d(),
        ProductBox::shadertoy_2d(64).unwrap(),
        ProductBox::extent(32.0, 16.0).unwrap(),
    ];
    for source in sources {
        let t = SpaceTransform::new(box_, Fit::DEFAULT, source).unwrap();
        for lamp in &lamps {
            let resolved = t.resolve(t.apply(*lamp), &OutsideRule::default());
            assert!(resolved.is_some(), "{source:?}: lamp {lamp:?} resolves");
        }
    }
    // Cover of a 2:1 extent on a 2:1 box is the extent exactly, y down.
    let fluid = SpaceTransform::new(box_, Fit::DEFAULT, sources[2]).unwrap();
    assert_eq!(at(&fluid, [200.0, 100.0]), q16s(32.0, 16.0));
    assert_eq!(at(&fluid, [0.0, 0.0]), q16s(0.0, 0.0));
}

/// `ViewBox::new` refuses a zero or negative width or height.
#[test]
fn a_view_box_of_no_width_or_height_is_refused() {
    for (w, h) in [
        (0.0, 1.0),
        (1.0, 0.0),
        (-1.0, 1.0),
        (1.0, -2.0),
        (f32::NAN, 1.0),
    ] {
        assert_eq!(
            ViewBox::new(0.0, 0.0, w, h),
            Err(SpaceError::EmptyViewBox),
            "{w} × {h}"
        );
    }
    assert!(ViewBox::new(-5.0, -5.0, 1.0, 1.0).is_ok());
    assert_eq!(
        ProductBox::shadertoy_2d(0),
        Err(SpaceError::EmptyProductBox)
    );
    let bad_scale = SpaceTransform::new(
        view_box(0.0, 0.0, 1.0, 1.0),
        Fit::DEFAULT.with_scale(0.0),
        ProductBox::unit_2d(),
    );
    assert_eq!(bad_scale, Err(SpaceError::BadScale));
}

/// `shadertoy`'s view for a 3:1 box is 192 × 64: the box's shape, 64 on
/// the short side.
#[test]
fn shadertoys_view_for_a_three_to_one_box_is_192_by_64() {
    let view = ProductBox::shadertoy_for(Some(&view_box(0.0, 0.0, 300.0, 100.0)), DEFAULT_VIEW_N);
    assert_eq!(view.map(|v| (v.width(), v.height())), Ok((192.0, 64.0)));
}

/// `shadertoy`'s view for a 1:3 box is 64 × 192.
#[test]
fn shadertoys_view_for_a_one_to_three_box_is_64_by_192() {
    let view = ProductBox::shadertoy_for(Some(&view_box(5.0, 5.0, 10.0, 30.0)), DEFAULT_VIEW_N);
    assert_eq!(view.map(|v| (v.width(), v.height())), Ok((64.0, 192.0)));
}

/// `shadertoy`'s view for a square box is 64 × 64.
#[test]
fn shadertoys_view_for_a_square_box_is_64_by_64() {
    let view = ProductBox::shadertoy_for(Some(&view_box(0.0, 0.0, 500.0, 500.0)), DEFAULT_VIEW_N);
    assert_eq!(view.map(|v| (v.width(), v.height())), Ok((64.0, 64.0)));
}

/// With no fixture behind the render, `shadertoy`'s view is 64 × 64.
#[test]
fn shadertoys_view_with_no_fixture_is_64_by_64() {
    let view = ProductBox::shadertoy_for(None, DEFAULT_VIEW_N);
    assert_eq!(view.map(|v| (v.width(), v.height())), Ok((64.0, 64.0)));
}

/// Under fill, a box-shaped `shadertoy` view maps the box's corners to its
/// own: `pos / outputSize` is the lamp's place in the box.
#[test]
fn a_box_shaped_shadertoy_view_under_fill_maps_corner_to_corner() {
    let box_ = view_box(0.0, 0.0, 300.0, 100.0);
    let view = ProductBox::shadertoy_for(Some(&box_), DEFAULT_VIEW_N).unwrap();
    let t = SpaceTransform::new(box_, Fit::new(FitMode::Fill, FitAlign::CENTRE), view).unwrap();
    assert_eq!(
        at(&t, [300.0, 0.0]),
        q16s(192.0, 64.0),
        "top-right is (192, 64)"
    );
    assert_eq!(
        at(&t, [0.0, 100.0]),
        q16s(0.0, 0.0),
        "bottom-left is the origin"
    );
}

// ---- helpers ------------------------------------------------------------------

fn view_box(min_x: f32, min_y: f32, width: f32, height: f32) -> ViewBox {
    ViewBox::new(min_x, min_y, width, height).unwrap()
}

fn unit(box_: ViewBox, fit: Fit) -> SpaceTransform {
    SpaceTransform::new(box_, fit, ProductBox::unit_2d()).unwrap()
}

fn shadertoy(box_: ViewBox, fit: Fit, n: u32) -> SpaceTransform {
    SpaceTransform::new(box_, fit, ProductBox::shadertoy_2d(n).unwrap()).unwrap()
}

/// The source coordinate of a lamp that must be inside the box.
fn at(t: &SpaceTransform, point: [f32; 2]) -> [i32; 2] {
    match t.apply(point) {
        Sample::At(q) => q,
        other => panic!("{point:?} is {other:?}"),
    }
}

fn q16(value: f32) -> i32 {
    (f64::from(value) * 65536.0).round() as i32
}

fn q16s(x: f64, y: f64) -> [i32; 2] {
    [(x * 65536.0).round() as i32, (y * 65536.0).round() as i32]
}

/// The centre of texel `k`, Q16.16 pixels.
fn texel(k: i32) -> i32 {
    (k << 16) + Q16_ONE / 2
}

/// Cover, centred, of the unit view on the lamps' own bounds, with the
/// lamps given in Q16.16 pixels as fixture-space numbers. An axis of no
/// size is padded by `pad` (in the same units) about its centre.
fn lamp_box_cover(lamps: &[[i32; 2]], pad: i32) -> SpaceTransform {
    let (min, max) = int_bounds(lamps);
    let size = |axis: usize| (max[axis] - min[axis]) as f32;
    let (mut y0, mut h) = (min[1] as f32, size(1));
    if h == 0.0 {
        y0 -= pad as f32 / 2.0;
        h = pad as f32;
    }
    unit(view_box(min[0] as f32, y0, size(0), h), Fit::DEFAULT)
}

fn int_bounds(lamps: &[[i32; 2]]) -> ([i32; 2], [i32; 2]) {
    let mut min = [i32::MAX; 2];
    let mut max = [i32::MIN; 2];
    for p in lamps {
        for axis in 0..2 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    (min, max)
}

/// The choker's lamps as the engine streams them to a pixel shader today:
/// the map2d's lamps contain-fitted into the fixture's 104 × 28 render
/// (`fit_points_in_place`, framed by the view box), truncated to Q16.16 and
/// scaled to pixels (`lpc-engine`'s `normalized_f32_to_q16`,
/// `normalized_q16_to_pixel_q16`).
fn choker_pixel_lamps() -> Vec<[i32; 2]> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../catalog/projects/playful-choker/playful.map2d.json"
    );
    let doc = lpc_mapping::map2d6::read(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut points = lpc_mapping::part::render(&doc)
        .unwrap()
        .positions()
        .to_vec();
    let [x, y, w, h] = doc.view_box;
    let frame = lpc_mapping::Bounds2d {
        min_x: x,
        min_y: y,
        width: w,
        height: h,
    };
    lpc_mapping::fit_points_in_place(&mut points, Some(frame), 104, 28).unwrap();
    points
        .iter()
        .map(|&[x, y]| {
            [
                (x.clamp(0.0, 1.0) * 65536.0) as i32 * 104,
                (y.clamp(0.0, 1.0) * 65536.0) as i32 * 28,
            ]
        })
        .collect()
}

/// Today's pattern frame, copied from `lpc-engine`'s `pattern_space.rs`
/// (`ScopeGeometry`'s box, `PatternFrame::two_d`, `PatternAxis`) at
/// `f90a0388a`, the reference T must reproduce.
struct TodayFrame {
    centre: [i32; 2],
    mul: i64,
    shift: u32,
    extent: [f32; 2],
}

impl TodayFrame {
    fn new(lamps: &[[i32; 2]]) -> Self {
        let (min, max) = int_bounds(lamps);
        let span = |axis: usize| (max[axis].wrapping_sub(min[axis])) as f32 / 65536.0 / 2.0;
        let centre = |axis: usize| ((i64::from(min[axis]) + i64::from(max[axis])) >> 1) as i32;
        let half = [span(0), span(1)];
        let half_long = half[0].max(half[1]);
        let scale = 1.0 / half_long;
        let extent = if half[0] >= half[1] {
            [1.0, half[1] * scale]
        } else {
            [half[0] * scale, 1.0]
        };
        let bits = scale.to_bits();
        let exponent = ((bits >> 23) & 0xff) as i32 - 127;
        let mantissa = i64::from((bits & 0x007F_FFFF) | 0x0080_0000);
        Self {
            centre: [centre(0), centre(1)],
            mul: mantissa << 7,
            shift: (30 - exponent).max(1) as u32,
            extent,
        }
    }

    fn map(&self, pixel: [i32; 2]) -> [i32; 2] {
        let axis = |value: i32, origin: i32, negate: bool| {
            let delta = i64::from(value) - i64::from(origin);
            let rounded = (delta * self.mul + (1i64 << (self.shift - 1))) >> self.shift;
            (if negate { -rounded } else { rounded }) as i32
        };
        [
            axis(pixel[0], self.centre[0], false),
            axis(pixel[1], self.centre[1], true),
        ]
    }

    fn extent(&self) -> [f32; 2] {
        self.extent
    }
}
