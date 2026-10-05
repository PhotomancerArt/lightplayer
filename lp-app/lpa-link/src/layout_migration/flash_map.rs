//! A plan's steps applied to a flash image held in memory.
//!
//! What the fake provider's board does with a plan, and what the planner's
//! own tests interrupt at every step. A `Write` replaces bytes the way a
//! flasher's write does (it erases the sectors it writes first); an `Erase`
//! fills with `0xff`.

use super::migration_plan::FlashStep;

/// Why a step could not be applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StepError {
    /// The step reaches past the end of the chip.
    OutOfRange { offset: u32, length: u32 },
    /// A readback did not match; `index` is the step's position.
    VerifyMismatch { index: usize, offset: u32 },
}

impl core::fmt::Display for StepError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutOfRange { offset, length } => {
                write!(f, "{offset:#x} + {length:#x} is past the end of the flash")
            }
            Self::VerifyMismatch { offset, .. } => {
                write!(
                    f,
                    "the files written at {offset:#x} did not read back the same"
                )
            }
        }
    }
}

/// Apply `steps` to `flash` in order. `firmware` is the merged image
/// [`FlashStep::WriteFirmware`] writes at `0x0`.
pub fn apply_steps(
    flash: &mut [u8],
    steps: &[FlashStep],
    firmware: &[u8],
) -> Result<(), StepError> {
    for (index, step) in steps.iter().enumerate() {
        match step {
            FlashStep::WriteFirmware => write(flash, 0, firmware)?,
            FlashStep::Erase { offset, length } => {
                range(flash.len(), *offset, *length)?;
                flash[*offset as usize..(*offset + *length) as usize].fill(0xFF);
            }
            FlashStep::Write { offset, bytes } => write(flash, *offset, bytes)?,
            FlashStep::VerifyEquals { offset, bytes } => {
                range(flash.len(), *offset, bytes.len() as u32)?;
                let at = *offset as usize;
                if flash[at..at + bytes.len()] != bytes[..] {
                    return Err(StepError::VerifyMismatch {
                        index,
                        offset: *offset,
                    });
                }
            }
        }
    }
    Ok(())
}

fn write(flash: &mut [u8], offset: u32, bytes: &[u8]) -> Result<(), StepError> {
    range(flash.len(), offset, bytes.len() as u32)?;
    let at = offset as usize;
    flash[at..at + bytes.len()].copy_from_slice(bytes);
    Ok(())
}

fn range(len: usize, offset: u32, length: u32) -> Result<(), StepError> {
    if (offset as usize).saturating_add(length as usize) > len {
        return Err(StepError::OutOfRange { offset, length });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mismatched_readback_names_its_step() {
        let mut flash = vec![0xFFu8; 0x10000];
        let steps = vec![
            FlashStep::Write {
                offset: 0x1000,
                bytes: vec![1; 0x1000],
            },
            FlashStep::VerifyEquals {
                offset: 0x1000,
                bytes: vec![2; 0x1000],
            },
        ];
        assert_eq!(
            apply_steps(&mut flash, &steps, &[]),
            Err(StepError::VerifyMismatch {
                index: 1,
                offset: 0x1000
            })
        );
    }

    #[test]
    fn a_step_past_the_chip_is_refused() {
        let mut flash = vec![0xFFu8; 0x1000];
        let steps = vec![FlashStep::Erase {
            offset: 0x1000,
            length: 0x1000,
        }];
        assert!(matches!(
            apply_steps(&mut flash, &steps, &[]),
            Err(StepError::OutOfRange { .. })
        ));
    }
}
