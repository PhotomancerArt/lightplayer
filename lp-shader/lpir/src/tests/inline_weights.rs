//! Candidate inline weight metrics (M3.1).

use crate::inline_weights::{weight, weight_body_len, weight_heavy_bias, weight_markers_zero, WeightKind};
use crate::parse::parse_module;
use crate::validate::validate_module;

const HANDCRAFTED: &str = r#"import @glsl::fsin(f32) -> f32

func @handcrafted(v1:f32) -> f32 {
  slot ss0, 8
  v2:i32 = slot_addr ss0
  v3:i32 = iconst.i32 0
  v4:f32 = fconst.f32 1.0
  v5:i32 = flt v1, v4
  if v5 {
    v6:f32 = fsqrt v1
    v7:f32 = call @glsl::fsin(v6)
    return v7
  } else {
    memcpy v2, v3, 8
    return v1
  }
}
"#;

#[test]
fn handcrafted_three_weights_and_dispatcher() {
    let m = parse_module(HANDCRAFTED).expect("parse");
    validate_module(&m).expect("validate");
    let f = m.functions.values().find(|g| g.name == "handcrafted").expect("func");

    let bl = weight_body_len(f);
    let mz = weight_markers_zero(f);
    let hb = weight_heavy_bias(f);

    assert_eq!(bl, 12, "body_len");
    assert_eq!(mz, 7, "markers_zero");
    assert_eq!(hb, 17, "heavy_bias");

    assert_eq!(weight(WeightKind::BodyLen, f), bl);
    assert_eq!(weight(WeightKind::MarkersZero, f), mz);
    assert_eq!(weight(WeightKind::HeavyBias, f), hb);
}
