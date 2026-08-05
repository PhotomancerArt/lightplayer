//! Baking a [`Gradient`] into the height-one texture a `sampler2D` palette
//! uniform reads.
//!
//! `lp-shader` deliberately does **not** bake palettes
//! (`docs/design/lp-shader-texture-access.md`, "Palette stop baking is out of
//! scope"): it defines sampling primitives, and higher layers turn authored
//! stops into texels. This is that higher layer.
//!
//! # The strip contract
//!
//! [`PALETTE_BAKE_WIDTH`] × 1 [`PALETTE_BAKE_FORMAT`] texels, sampled with
//! `Linear` filter and `Repeat` wrap on X. Texel `i` holds the gradient at
//! **`t = (i + 0.5) / WIDTH`** — the texel *center*, which is what makes
//! `texture(palette, vec2(u, 0))` return the gradient at `u` exactly rather
//! than half a texel off. It also makes the `Repeat` seam do the right thing:
//! sampling at `u = 0` blends texel `WIDTH-1` (`t ≈ 1`) with texel `0`
//! (`t ≈ 0`), so a gradient authored to wrap does, and one authored not to
//! shows its join only in the half-texel either side of the seam.
//!
//! # Where range is lost
//!
//! Interpolation and conversion keep out-of-gamut coordinates (`color.md`
//! §10 rule 6), and this is the boundary that cannot: `Rgba16Unorm` stores
//! `[0,1]`. Clamping happens here, once, at the texture-write boundary
//! §7 names — never earlier, where it would silently change what an Oklch
//! interpolation passes through.

use lpc_model::{Gradient, GradientStop};
use lps_shared::TextureStorageFormat;

use super::colorspace::{interpolate_in_space, to_linear_srgb};

/// Texels in one baked palette strip.
///
/// 256 is the D3 sub-lean: one texel per 8-bit position, so an imported
/// 256-entry WLED-style palette bakes without resampling, and the strip is
/// still only [`PALETTE_BAKE_BYTES`] — small enough that a device can hold a
/// few without noticing.
pub const PALETTE_BAKE_WIDTH: u32 = 256;

/// Storage format of a baked palette strip. `Rgba16Unorm` is the one format
/// that supports *filtered* sampling and carries the precision canonical
/// LinearSrgb needs (`Rgb16Unorm` is `texelFetch`-only — see the texture
/// access design's format table).
pub const PALETTE_BAKE_FORMAT: TextureStorageFormat = TextureStorageFormat::Rgba16Unorm;

/// Bytes in one baked palette strip: 256 texels × RGBA × `u16`.
pub const PALETTE_BAKE_BYTES: usize = PALETTE_BAKE_WIDTH as usize * 4 * 2;

/// Bake `gradient` into `out`, which must be [`PALETTE_BAKE_BYTES`] long.
///
/// Panics only on a wrong-length buffer, which is a caller bug rather than
/// authored data — every authored path goes through the constant.
pub fn bake_gradient_into(gradient: &Gradient, out: &mut [u8]) {
    // The stop order is resolved ONCE for the whole strip, not per texel.
    // Doing it inside the loop cost ~2.5× the bake: `StopOrder` is a
    // fixed-size 24-stop buffer, so sorting it 256 times moved ~100 KB per
    // strip for a two-stop gradient. Same below for the mix bake.
    let order = stop_order(gradient);
    bake(out, |t| order.sample(gradient, t));
}

/// Bake the cross-fade between two gradients: `from` at `mix = 0`, `to` at
/// `mix = 1`.
///
/// The blend happens in **canonical LinearSrgb**, after each side has been
/// interpolated and converted in its own authoring space. That is the only
/// choice that is well defined when the two sides disagree about space —
/// which a palette cycle's set routinely does — and it matches what the
/// cross-fade means physically: two strips dissolved into one another.
pub fn bake_gradient_mix_into(from: &Gradient, to: &Gradient, mix: f32, out: &mut [u8]) {
    let mix = if mix.is_finite() {
        mix.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let from_order = stop_order(from);
    let to_order = stop_order(to);
    bake(out, |t| {
        let a = from_order.sample(from, t);
        let b = to_order.sample(to, t);
        [
            a[0] + (b[0] - a[0]) * mix,
            a[1] + (b[1] - a[1]) * mix,
            a[2] + (b[2] - a[2]) * mix,
        ]
    });
}

/// Sample `gradient` at `t ∈ [0,1]`, in canonical LinearSrgb.
///
/// Stops are read in `at` order without being reordered in place:
/// [`Gradient::validate`] deliberately does not enforce authored order, so
/// resolving has to sort — and does it in a fixed-size index buffer rather
/// than by allocating, because this runs 256 times per bake on a device.
///
/// Outside the authored span the endpoints hold (clamp, never extrapolate):
/// a gradient whose first stop is at `0.25` is flat below `0.25`.
#[must_use]
pub fn sample_gradient(gradient: &Gradient, t: f32) -> [f32; 3] {
    stop_order(gradient).sample(gradient, t)
}

/// The two stops bracketing `t`, and `t`'s normalized position between them.
fn segment_at(stops: &[GradientStop], t: f32) -> (GradientStop, GradientStop, f32) {
    let first = stops[0];
    let last = stops[stops.len() - 1];
    if t <= first.at {
        return (first, first, 0.0);
    }
    if t >= last.at {
        return (last, last, 0.0);
    }
    for window in stops.windows(2) {
        let (from, to) = (window[0], window[1]);
        if t <= to.at {
            let span = to.at - from.at;
            // Coincident stops are a legal way to author a hard edge; the
            // segment is then zero-wide and `t` lands on its start.
            let local = if span > 0.0 {
                (t - from.at) / span
            } else {
                0.0
            };
            return (from, to, local);
        }
    }
    (last, last, 0.0)
}

/// Stops sorted by `at`, held in a fixed-size buffer.
///
/// [`lpc_model::MAX_GRADIENT_STOPS`] bounds the count, so this never
/// allocates; insertion sort because 24 is small and the authored order is
/// almost always already sorted.
struct StopOrder {
    sorted: [GradientStop; lpc_model::MAX_GRADIENT_STOPS as usize],
    len: usize,
}

impl StopOrder {
    /// Sample the gradient these stops came from at `t`, in canonical
    /// LinearSrgb. `gradient` supplies only `space` and `method` — the stops
    /// are the sorted ones held here.
    fn sample(&self, gradient: &Gradient, t: f32) -> [f32; 3] {
        match &self.sorted[..self.len] {
            // A gradient below `MIN_GRADIENT_STOPS` is not authorable through
            // `validate`, but a def can be built by hand; black is the honest
            // answer, and it is the same one the black fallback gives.
            [] => [0.0, 0.0, 0.0],
            [only] => to_linear_srgb(gradient.space, only.c),
            stops => {
                let t = if t.is_finite() {
                    t.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (from, to, local) = segment_at(stops, t);
                to_linear_srgb(
                    gradient.space,
                    interpolate_in_space(gradient.space, gradient.method, from.c, to.c, local),
                )
            }
        }
    }
}

fn stop_order(gradient: &Gradient) -> StopOrder {
    let mut sorted = [GradientStop::default(); lpc_model::MAX_GRADIENT_STOPS as usize];
    let mut len = 0;
    for stop in gradient
        .stops
        .iter()
        .take(lpc_model::MAX_GRADIENT_STOPS as usize)
    {
        // A non-finite position cannot be ordered; dropping it keeps the rest
        // of the gradient renderable, which the black fallback would not.
        if !stop.at.is_finite() {
            continue;
        }
        let mut index = len;
        while index > 0 && sorted[index - 1].at > stop.at {
            sorted[index] = sorted[index - 1];
            index -= 1;
        }
        sorted[index] = *stop;
        len += 1;
    }
    StopOrder { sorted, len }
}

/// Write [`PALETTE_BAKE_WIDTH`] texel centers through `color`.
fn bake(out: &mut [u8], color: impl Fn(f32) -> [f32; 3]) {
    assert_eq!(
        out.len(),
        PALETTE_BAKE_BYTES,
        "palette bake buffer must be exactly one strip"
    );
    for index in 0..PALETTE_BAKE_WIDTH as usize {
        let t = (index as f32 + 0.5) / PALETTE_BAKE_WIDTH as f32;
        let rgb = color(t);
        let base = index * 8;
        for lane in 0..3 {
            let bytes = unorm16(rgb[lane]).to_le_bytes();
            out[base + lane * 2] = bytes[0];
            out[base + lane * 2 + 1] = bytes[1];
        }
        // Palettes are opaque: alpha exists because the format has it, not
        // because a gradient stop can author it.
        out[base + 6] = 0xff;
        out[base + 7] = 0xff;
    }
}

/// Canonical LinearSrgb F32 → linear Unorm16, round-to-nearest.
///
/// The one clamp in the palette path (see the module docs).
fn unorm16(value: f32) -> u16 {
    if !value.is_finite() {
        return 0;
    }
    let scaled = value.clamp(0.0, 1.0) * u16::MAX as f32 + 0.5;
    scaled as u16
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use lpc_model::{Colorspace, InterpMethod};

    use super::*;

    #[test]
    fn a_bake_is_exactly_one_strip_of_opaque_texels() {
        let mut out = vec![0u8; PALETTE_BAKE_BYTES];
        bake_gradient_into(&black_to_white(), &mut out);

        assert_eq!(out.len(), PALETTE_BAKE_WIDTH as usize * 8);
        for index in 0..PALETTE_BAKE_WIDTH as usize {
            assert_eq!(
                (out[index * 8 + 6], out[index * 8 + 7]),
                (0xff, 0xff),
                "texel {index} alpha"
            );
        }
    }

    #[test]
    fn texel_centers_carry_the_gradient_at_their_own_position() {
        let gradient = Gradient {
            space: Colorspace::LinearSrgb,
            method: InterpMethod::Linear,
            stops: vec![
                GradientStop {
                    at: 0.0,
                    c: [0.0, 0.0, 0.0],
                },
                GradientStop {
                    at: 1.0,
                    c: [1.0, 1.0, 1.0],
                },
            ],
        };
        let mut out = vec![0u8; PALETTE_BAKE_BYTES];
        bake_gradient_into(&gradient, &mut out);

        for index in [0usize, 64, 128, 255] {
            let t = (index as f32 + 0.5) / PALETTE_BAKE_WIDTH as f32;
            let expected = unorm16(t);
            assert_eq!(texel(&out, index)[0], expected, "texel {index}");
        }
    }

    #[test]
    fn a_step_gradient_bakes_hard_edges_and_never_blends() {
        let gradient = Gradient {
            space: Colorspace::LinearSrgb,
            method: InterpMethod::Step,
            stops: vec![
                GradientStop {
                    at: 0.0,
                    c: [1.0, 0.0, 0.0],
                },
                GradientStop {
                    at: 0.5,
                    c: [0.0, 1.0, 0.0],
                },
                GradientStop {
                    at: 1.0,
                    c: [0.0, 0.0, 1.0],
                },
            ],
        };
        let mut out = vec![0u8; PALETTE_BAKE_BYTES];
        bake_gradient_into(&gradient, &mut out);

        // Every texel is one of the three authored colors, never a mix.
        for index in 0..PALETTE_BAKE_WIDTH as usize {
            let texel = texel(&out, index);
            let lit = texel.iter().take(3).filter(|c| **c > 0).count();
            assert_eq!(lit, 1, "texel {index} is a blend: {texel:?}");
            assert_eq!(texel.iter().take(3).copied().max(), Some(u16::MAX));
        }
        assert_eq!(texel(&out, 0)[0], u16::MAX, "below 0.5 is the first stop");
        assert_eq!(texel(&out, 200)[1], u16::MAX, "at/above 0.5 is the second");
    }

    #[test]
    fn stops_resolve_in_position_order_however_they_were_authored() {
        let mut shuffled = black_to_white();
        shuffled.stops.reverse();

        let mut ordered_out = vec![0u8; PALETTE_BAKE_BYTES];
        let mut shuffled_out = vec![0u8; PALETTE_BAKE_BYTES];
        bake_gradient_into(&black_to_white(), &mut ordered_out);
        bake_gradient_into(&shuffled, &mut shuffled_out);

        assert_eq!(ordered_out, shuffled_out);
    }

    #[test]
    fn outside_the_authored_span_the_endpoints_hold() {
        let gradient = Gradient {
            space: Colorspace::LinearSrgb,
            method: InterpMethod::Linear,
            stops: vec![
                GradientStop {
                    at: 0.25,
                    c: [1.0, 0.0, 0.0],
                },
                GradientStop {
                    at: 0.75,
                    c: [0.0, 0.0, 1.0],
                },
            ],
        };

        assert_eq!(sample_gradient(&gradient, 0.0), [1.0, 0.0, 0.0]);
        assert_eq!(sample_gradient(&gradient, 0.1), [1.0, 0.0, 0.0]);
        assert_eq!(sample_gradient(&gradient, 1.0), [0.0, 0.0, 1.0]);
        // No extrapolation past the ends, even for a wild `t`.
        assert_eq!(sample_gradient(&gradient, 12.0), [0.0, 0.0, 1.0]);
        assert_eq!(sample_gradient(&gradient, f32::NAN), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn interpolation_happens_in_the_authored_space() {
        // The midpoint of an sRGB black→white ramp is sRGB 0.5, whose linear
        // value is ~0.214 — NOT the 0.5 a LinearSrgb ramp would give. That
        // difference is the whole reason `color.md` §6 interpolates in the
        // authored space.
        let srgb = Gradient {
            space: Colorspace::Srgb,
            method: InterpMethod::Linear,
            stops: vec![
                GradientStop {
                    at: 0.0,
                    c: [0.0, 0.0, 0.0],
                },
                GradientStop {
                    at: 1.0,
                    c: [1.0, 1.0, 1.0],
                },
            ],
        };
        let mid = sample_gradient(&srgb, 0.5);
        assert!((mid[0] - 0.214_041).abs() < 1e-4, "{mid:?}");
        assert!((sample_gradient(&black_to_white(), 0.5)[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_mix_bake_dissolves_between_two_strips_in_canonical_space() {
        let mut from_out = vec![0u8; PALETTE_BAKE_BYTES];
        let mut to_out = vec![0u8; PALETTE_BAKE_BYTES];
        let mut mid_out = vec![0u8; PALETTE_BAKE_BYTES];
        let red = solid(Colorspace::LinearSrgb, [1.0, 0.0, 0.0]);
        let blue = solid(Colorspace::LinearSrgb, [0.0, 0.0, 1.0]);

        bake_gradient_into(&red, &mut from_out);
        bake_gradient_into(&blue, &mut to_out);
        bake_gradient_mix_into(&red, &blue, 0.5, &mut mid_out);

        assert_eq!(texel(&mid_out, 10)[0], unorm16(0.5));
        assert_eq!(texel(&mid_out, 10)[2], unorm16(0.5));

        // The ends of the mix are bit-identical to the plain bakes, so a
        // fade that reaches 0 or 1 lands exactly on a cached static bake.
        let mut ends = vec![0u8; PALETTE_BAKE_BYTES];
        bake_gradient_mix_into(&red, &blue, 0.0, &mut ends);
        assert_eq!(ends, from_out);
        bake_gradient_mix_into(&red, &blue, 1.0, &mut ends);
        assert_eq!(ends, to_out);
    }

    #[test]
    fn out_of_gamut_coordinates_clamp_at_the_texture_boundary_only() {
        let hot = solid(Colorspace::LinearSrgb, [2.0, -1.0, 0.5]);
        // The sample itself keeps the overshoot...
        assert_eq!(sample_gradient(&hot, 0.5), [2.0, -1.0, 0.5]);

        // ...and only the texel is clamped to the storage grid.
        let mut out = vec![0u8; PALETTE_BAKE_BYTES];
        bake_gradient_into(&hot, &mut out);
        assert_eq!(texel(&out, 0)[0], u16::MAX);
        assert_eq!(texel(&out, 0)[1], 0);
    }

    #[test]
    fn a_hand_built_gradient_with_too_few_stops_bakes_black_rather_than_panicking() {
        let empty = Gradient {
            space: Colorspace::Srgb,
            method: InterpMethod::Linear,
            stops: Vec::new(),
        };
        assert_eq!(sample_gradient(&empty, 0.5), [0.0, 0.0, 0.0]);

        let one = solid(Colorspace::LinearSrgb, [0.25, 0.25, 0.25]);
        let single = Gradient {
            stops: vec![one.stops[0]],
            ..one
        };
        assert_eq!(sample_gradient(&single, 0.9), [0.25, 0.25, 0.25]);
    }

    fn black_to_white() -> Gradient {
        Gradient {
            space: Colorspace::LinearSrgb,
            method: InterpMethod::Linear,
            stops: vec![
                GradientStop {
                    at: 0.0,
                    c: [0.0, 0.0, 0.0],
                },
                GradientStop {
                    at: 1.0,
                    c: [1.0, 1.0, 1.0],
                },
            ],
        }
    }

    fn solid(space: Colorspace, c: [f32; 3]) -> Gradient {
        Gradient {
            space,
            method: InterpMethod::Linear,
            stops: vec![GradientStop { at: 0.0, c }, GradientStop { at: 1.0, c }],
        }
    }

    fn texel(bytes: &[u8], index: usize) -> [u16; 4] {
        let base = index * 8;
        let mut out = [0u16; 4];
        for lane in 0..4 {
            out[lane] = u16::from_le_bytes([bytes[base + lane * 2], bytes[base + lane * 2 + 1]]);
        }
        out
    }
}
