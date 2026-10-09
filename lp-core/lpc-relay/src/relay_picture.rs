//! A board's picture: what its lamps show, sampled, as it sends it to the
//! hub ([`RelayFrame::Picture`](crate::RelayFrame::Picture), protocol 2).

use alloc::vec::Vec;

use crate::frame_reader::FrameReader;
use crate::relay_frame::RelayFrameError;
use crate::relay_limits::{MAX_PICTURE_OUTPUTS, MAX_RELAY_FRAME};

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
        out.push(outputs.len() as u8);
        for lamps in outputs {
            out.extend_from_slice(&lamps.to_le_bytes());
        }
        let count = self.samples().min(usize::from(u16::MAX));
        out.extend_from_slice(&(count as u16).to_le_bytes());
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
}
