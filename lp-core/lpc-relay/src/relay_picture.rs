//! A board's picture: what its lamps show, sampled, as it sends it to the
//! hub ([`RelayFrame::Picture`](crate::RelayFrame::Picture), protocol 2).
//!
//! Two ways to make the frame: [`RelayFrame::encode`](crate::RelayFrame::encode)
//! of a [`RelayPicture`] (the hub, tests), or in place on a board
//! ([`write_picture_header`], then the colours appended behind it), so an
//! edge fills one buffer it keeps and never builds a `RelayPicture`. Both
//! write the same bytes.

use alloc::vec::Vec;

use crate::frame_reader::FrameReader;
use crate::relay_frame::{RelayFrameError, TAG_PICTURE};
use crate::relay_limits::{MAX_PICTURE_OUTPUTS, MAX_RELAY_FRAME};

/// How many samples a board sends for `lamps_total` lamps, at most `max`:
/// `min(T, max)` (and never more than a frame's `u16` count), so 0 exactly
/// when `T` is 0. A board passes
/// [`DEFAULT_PICTURE_SAMPLES`](crate::DEFAULT_PICTURE_SAMPLES).
#[must_use]
pub fn picture_sample_count(lamps_total: u64, max: usize) -> u16 {
    let max = u64::try_from(max).unwrap_or(u64::MAX);
    u16::try_from(lamps_total.min(max)).unwrap_or(u16::MAX)
}

/// Write a [`RelayFrame::Picture`](crate::RelayFrame::Picture) frame's tag,
/// outputs and count into `out` (cleared first, its capacity kept); the
/// caller then appends `count × 3` colour bytes, R, G, B per sample, in the
/// meaning [`RelayPicture`] gives them. The finished buffer is
/// byte-identical to `RelayFrame::Picture(..).encode()` of the same
/// picture.
///
/// Refuses what the hub's decoder would: more than
/// [`MAX_PICTURE_OUTPUTS`] outputs, or a `count` the lamps contradict
/// ([`RelayFrameError::BadField`]); a frame past [`MAX_RELAY_FRAME`] once
/// its colours are in ([`RelayFrameError::TooLong`]). On a refusal `out`
/// is left empty.
pub fn write_picture_header(
    out: &mut Vec<u8>,
    lamps: &[u32],
    count: u16,
) -> Result<(), RelayFrameError> {
    out.clear();
    check_shape(lamps, usize::from(count))?;
    if 1 + 1 + 4 * lamps.len() + 2 + 3 * usize::from(count) > MAX_RELAY_FRAME {
        return Err(RelayFrameError::TooLong);
    }
    out.push(TAG_PICTURE);
    put_header(out, lamps, count);
    Ok(())
}

/// `n u8`, `n × lamps u32`, `count u16`: a picture's fields before its
/// colours. `lamps` holds at most [`MAX_PICTURE_OUTPUTS`] entries.
fn put_header(out: &mut Vec<u8>, lamps: &[u32], count: u16) {
    out.push(lamps.len() as u8);
    for lamps in lamps {
        out.extend_from_slice(&lamps.to_le_bytes());
    }
    out.extend_from_slice(&count.to_le_bytes());
}

/// The colours a board's lamps show, point-sampled.
///
/// **What it means.** The board's outputs (at most
/// [`MAX_PICTURE_OUTPUTS`]) are concatenated in the project's tree order;
/// `T` is their lamps' sum ([`Self::lamps`]) and `count` the samples
/// ([`Self::samples`]). Sample `i` is lamp `⌊i·T/count⌋` of that
/// concatenation ([`Self::sample_lamp`]). `count` is 0 exactly when `T` is
/// 0, and otherwise `1 ≤ count ≤ T`.
///
/// Each sample is three bytes, **R, G, B**: 8-bit sRGB display codes, the
/// colours Studio's card draws for those lamps. A 16-bit output sample is
/// encoded to sRGB8 (`linear16_to_srgb8`); an 8-bit one is widened by 257
/// first and encoded the same way, so a picture's codes mean one thing
/// whatever the output's buffer holds. The colours are the published ones,
/// after finalize (brightness, gamma and the power limit as the output
/// sends them), with the output's colour order undone; a lamp no
/// `RgbPixels` span of the output covers is read in wire order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayPicture {
    /// Lamps per output, in the project's tree order (at most
    /// [`MAX_PICTURE_OUTPUTS`]).
    pub outputs: Vec<u32>,
    /// `count` samples, three bytes each: R, G, B.
    pub colors: Vec<u8>,
}

impl RelayPicture {
    /// The lamps of every output together: `T`. Exact (a sum of `u32`s in
    /// a `u64`); a picture the hub accepts has `T ≤ u32::MAX`.
    #[must_use]
    pub fn lamps(&self) -> u64 {
        self.outputs.iter().map(|&lamps| u64::from(lamps)).sum()
    }

    /// How many samples the picture holds: `count`.
    #[must_use]
    pub fn samples(&self) -> usize {
        self.colors.len() / 3
    }

    /// The lamp sample `i` shows, counted across the outputs in tree order:
    /// `⌊i·T/count⌋`, computed in `u64`. This is the meaning of sample `i`;
    /// 0 for a picture with no samples.
    #[must_use]
    pub fn sample_lamp(&self, i: usize) -> u32 {
        let count = self.samples() as u64;
        if count == 0 {
            return 0;
        }
        let lamp = (i as u64).saturating_mul(self.lamps()) / count;
        u32::try_from(lamp).unwrap_or(u32::MAX)
    }

    /// Whether the hub would take this picture: the rules its decoder
    /// checks (outputs, lamp sum, sample count, three bytes a sample), and
    /// the frame within [`MAX_RELAY_FRAME`]. A sender checks it before it
    /// sends; [`RelayFrame::encode`](crate::RelayFrame::encode) does not.
    pub fn validate(&self) -> Result<(), RelayFrameError> {
        if self.colors.len() % 3 != 0 || self.samples() > usize::from(u16::MAX) {
            return Err(RelayFrameError::BadField);
        }
        check_shape(&self.outputs, self.samples())?;
        if self.encoded_len() > MAX_RELAY_FRAME {
            return Err(RelayFrameError::TooLong);
        }
        Ok(())
    }

    /// The frame's length: the tag, `n`, the lamps, `count`, the colours.
    fn encoded_len(&self) -> usize {
        1 + 1 + 4 * self.outputs.len() + 2 + self.colors.len()
    }

    /// The fields' bytes after the tag: `n u8`, `n × lamps u32`,
    /// `count u16`, `count × [r g b]`. Infallible: more outputs than
    /// [`MAX_PICTURE_OUTPUTS`] are cut to the first sixteen, and the
    /// samples to whole ones, at most `u16::MAX`. A picture that breaks a
    /// rule still encodes, and the hub refuses it ([`Self::validate`]).
    pub(crate) fn put(&self, out: &mut Vec<u8>) {
        let outputs = &self.outputs[..self.outputs.len().min(MAX_PICTURE_OUTPUTS)];
        let count = self.samples().min(usize::from(u16::MAX));
        put_header(out, outputs, count as u16);
        out.extend_from_slice(&self.colors[..count * 3]);
    }

    /// Read the fields [`Self::put`] writes, checking every rule.
    pub(crate) fn read(r: &mut FrameReader<'_>) -> Result<Self, RelayFrameError> {
        let n = usize::from(r.u8()?);
        if n > MAX_PICTURE_OUTPUTS {
            return Err(RelayFrameError::BadField);
        }
        let mut outputs = Vec::with_capacity(n);
        for _ in 0..n {
            outputs.push(r.u32()?);
        }
        let count = usize::from(r.u16()?);
        check_shape(&outputs, count)?;
        let colors = r.take(count * 3)?.to_vec();
        Ok(Self { outputs, colors })
    }
}

/// The rules between a picture's outputs and its sample count: at most
/// [`MAX_PICTURE_OUTPUTS`] outputs, a lamp sum that fits a `u32`, `count`
/// 0 exactly when the sum is, and never more samples than lamps.
fn check_shape(outputs: &[u32], count: usize) -> Result<(), RelayFrameError> {
    if outputs.len() > MAX_PICTURE_OUTPUTS {
        return Err(RelayFrameError::BadField);
    }
    let lamps: u64 = outputs.iter().map(|&lamps| u64::from(lamps)).sum();
    if lamps > u64::from(u32::MAX) {
        return Err(RelayFrameError::BadField);
    }
    let count = count as u64;
    if (count == 0) != (lamps == 0) || count > lamps {
        return Err(RelayFrameError::BadField);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn a_sample_is_lamp_i_t_over_count_across_the_outputs() {
        // 5 + 3 lamps, 4 samples: lamps 0, 2, 4, 6.
        let picture = RelayPicture {
            outputs: vec![5, 3],
            colors: vec![0; 12],
        };
        assert_eq!(picture.lamps(), 8);
        assert_eq!(picture.samples(), 4);
        let lamps: Vec<u32> = (0..4).map(|i| picture.sample_lamp(i)).collect();
        assert_eq!(lamps, [0, 2, 4, 6]);

        // Every lamp sampled when count == T.
        let full = RelayPicture {
            outputs: vec![3],
            colors: vec![0; 9],
        };
        let lamps: Vec<u32> = (0..3).map(|i| full.sample_lamp(i)).collect();
        assert_eq!(lamps, [0, 1, 2]);
    }

    #[test]
    fn the_sample_lamp_does_not_overflow_on_a_big_picture() {
        let picture = RelayPicture {
            outputs: vec![u32::MAX],
            colors: vec![0; 3 * 660],
        };
        assert_eq!(
            picture.sample_lamp(659),
            (659 * u64::from(u32::MAX) / 660) as u32
        );
        assert_eq!(
            RelayPicture {
                outputs: vec![],
                colors: vec![]
            }
            .sample_lamp(3),
            0
        );
    }

    #[test]
    fn validate_holds_the_decoders_rules() {
        let ok = RelayPicture {
            outputs: vec![5, 3],
            colors: vec![0; 12],
        };
        assert_eq!(ok.validate(), Ok(()));
        assert_eq!(
            RelayPicture {
                outputs: vec![],
                colors: vec![]
            }
            .validate(),
            Ok(())
        );
        let bad = [
            RelayPicture {
                outputs: vec![1; 17],
                colors: vec![0; 3],
            },
            RelayPicture {
                outputs: vec![u32::MAX, 1],
                colors: vec![0; 3],
            },
            RelayPicture {
                outputs: vec![3],
                colors: vec![],
            },
            RelayPicture {
                outputs: vec![],
                colors: vec![0; 3],
            },
            RelayPicture {
                outputs: vec![0, 0],
                colors: vec![0; 3],
            },
            RelayPicture {
                outputs: vec![2],
                colors: vec![0; 9],
            },
            RelayPicture {
                outputs: vec![3],
                colors: vec![0; 4],
            },
        ];
        for picture in bad {
            assert_eq!(
                picture.validate(),
                Err(RelayFrameError::BadField),
                "{picture:?}"
            );
        }
        let too_long = RelayPicture {
            outputs: vec![1000; 16],
            colors: vec![0; 3 * 661],
        };
        assert_eq!(too_long.validate(), Err(RelayFrameError::TooLong));
        let at_limit = RelayPicture {
            outputs: vec![1000; 16],
            colors: vec![0; 3 * 660],
        };
        assert_eq!(at_limit.validate(), Ok(()));
    }

    /// The header written in place, then the colours behind it, is each
    /// `Picture` golden of `tests/relay_frame_golden_v2.rs` (the hex copied
    /// here as literals), and decodes back.
    #[test]
    fn the_header_written_in_place_makes_the_golden_frames() {
        let cases: [(&[u32], &[u8], &str); 3] = [
            (&[], &[], "0b 00 0000"),
            (
                &[3],
                &[0xff, 0, 0, 0, 0xff, 0, 0, 0, 0xff],
                "0b 01 03000000 0300 ff0000 00ff00 0000ff",
            ),
            (
                &[5, 3],
                &[
                    0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0,
                ],
                "0b 02 05000000 03000000 0400 102030 405060 708090 a0b0c0",
            ),
        ];
        // A reused buffer: whatever it held is cleared.
        let mut out = vec![0xaa; 7];
        for (lamps, colors, golden) in cases {
            let count = (colors.len() / 3) as u16;
            write_picture_header(&mut out, lamps, count).expect("a valid picture");
            out.extend_from_slice(colors);
            assert_eq!(out, unhex(golden), "{golden}");
            assert_eq!(
                crate::RelayFrame::decode(&out),
                Ok(crate::RelayFrame::Picture(RelayPicture {
                    outputs: lamps.to_vec(),
                    colors: colors.to_vec(),
                })),
                "{golden}"
            );
        }
    }

    #[test]
    fn the_header_writer_refuses_what_the_decoder_would() {
        let mut out = vec![1, 2, 3];
        assert_eq!(
            write_picture_header(&mut out, &[1; 17], 1),
            Err(RelayFrameError::BadField),
            "seventeen outputs"
        );
        assert!(out.is_empty(), "a refusal leaves the buffer empty");
        assert_eq!(
            write_picture_header(&mut out, &[3], 0),
            Err(RelayFrameError::BadField),
            "no samples of three lamps"
        );
        assert_eq!(
            write_picture_header(&mut out, &[], 1),
            Err(RelayFrameError::BadField),
            "a sample of no lamps"
        );
        assert_eq!(
            write_picture_header(&mut out, &[2], 3),
            Err(RelayFrameError::BadField),
            "more samples than lamps"
        );
        assert_eq!(
            write_picture_header(&mut out, &[1000; 16], 661),
            Err(RelayFrameError::TooLong)
        );
        assert_eq!(write_picture_header(&mut out, &[1000; 16], 660), Ok(()));
    }

    #[test]
    fn a_board_sends_at_most_its_cap_and_none_for_no_lamps() {
        use crate::relay_limits::{DEFAULT_PICTURE_SAMPLES, MAX_BOARD_PICTURE_FRAME};
        assert_eq!(picture_sample_count(0, DEFAULT_PICTURE_SAMPLES), 0);
        assert_eq!(picture_sample_count(73, DEFAULT_PICTURE_SAMPLES), 73);
        assert_eq!(picture_sample_count(256, DEFAULT_PICTURE_SAMPLES), 256);
        assert_eq!(picture_sample_count(1000, DEFAULT_PICTURE_SAMPLES), 256);
        assert_eq!(picture_sample_count(u64::MAX, usize::MAX), u16::MAX);
        // The biggest frame a board makes fits the buffer it reserves.
        let mut out = Vec::new();
        let count = picture_sample_count(16 * 1000, DEFAULT_PICTURE_SAMPLES);
        write_picture_header(&mut out, &[1000; 16], count).unwrap();
        out.resize(out.len() + 3 * usize::from(count), 0);
        assert_eq!(out.len(), MAX_BOARD_PICTURE_FRAME);
        assert_eq!(MAX_BOARD_PICTURE_FRAME, 836);
    }

    fn unhex(text: &str) -> Vec<u8> {
        let text: Vec<u8> = text.bytes().filter(|b| *b != b' ').collect();
        text.chunks(2)
            .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
}
