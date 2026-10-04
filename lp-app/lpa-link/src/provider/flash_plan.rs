//! An ordered list of flash writes a provider executes in one bootloader
//! session — the shape a layout migration hands an executor.
//!
//! The plan is DECIDED in [`crate::layout_migration`] (pure) and EXECUTED by
//! the host (espflash), browser (esptool-js) and fake providers, each the
//! same way: the steps in order, a [`FlashStep::VerifyEquals`] read back and
//! compared, and on a mismatch the filesystem tail ([`FlashPlan::lpfs_tail`])
//! re-run once before the operation fails naming the step. Why the steps
//! come in the order they do is `layout_migration::migration_plan`'s module
//! docs.

/// One step of a plan. Every offset and length is 4 KB aligned.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum FlashStep {
    /// Write the firmware package's merged image (bootloader, partition
    /// table, app) at `0x0` — the bytes the provider already holds for the
    /// build being flashed, so a plan never carries them.
    WriteFirmware,
    /// Erase `length` bytes at `offset`.
    Erase { offset: u32, length: u32 },
    /// Write `bytes` at `offset`.
    Write { offset: u32, bytes: Vec<u8> },
    /// Read `bytes.len()` bytes at `offset` back and compare.
    VerifyEquals { offset: u32, bytes: Vec<u8> },
}

impl FlashStep {
    /// The progress label a user sees while this step runs.
    pub fn label(&self) -> &'static str {
        match self {
            Self::WriteFirmware => "Writing firmware",
            Self::Erase { .. } | Self::Write { .. } => "Moving files",
            Self::VerifyEquals { .. } => "Verifying files",
        }
    }
}

/// An ordered list of steps, and what an executor must check first.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct FlashPlan {
    pub steps: Vec<FlashStep>,
    /// The steps destroy the only on-board copy of the files: an executor
    /// must refuse the plan until [`Self::backup_confirmed`] is set.
    pub requires_backup: bool,
    /// Set by the caller once the backup is stored and read back (or the
    /// user downloaded it).
    pub backup_confirmed: bool,
    /// Index of the first step of the filesystem write — what an executor
    /// re-runs once when a readback does not match.
    pub lpfs_start: usize,
    /// The board the plan was made for (its base MAC, lowercase colon hex),
    /// when the inspection read one. An executor refuses to run the plan on
    /// any other board: a different board plugged in between the inspection
    /// and the write would otherwise receive this one's files.
    pub base_mac: Option<String>,
}

impl FlashPlan {
    /// The filesystem-write tail, re-run once on a readback mismatch.
    pub fn lpfs_tail(&self) -> &[FlashStep] {
        &self.steps[self.lpfs_start.min(self.steps.len())..]
    }

    /// May an executor start this plan?
    pub fn may_execute(&self) -> bool {
        !self.requires_backup || self.backup_confirmed
    }

    /// Does the plan write a filesystem (a migration or a restore)?
    pub fn carries_files(&self) -> bool {
        self.requires_backup
    }

    /// Why this plan must not run on the board that reported `probed_mac`
    /// (as reported — normalized here), or `None` to proceed. A plan made
    /// for a known board refuses any other, and refuses a board that would
    /// not name itself.
    pub fn refuse_board(&self, probed_mac: Option<&str>) -> Option<String> {
        let expected = self.base_mac.as_deref()?;
        let actual = probed_mac.and_then(crate::normalize_base_mac);
        (actual.as_deref() != Some(expected)).then(|| {
            format!(
                "refusing to write: this update was prepared for the board {expected}, but the \
                 connected board is {}",
                actual
                    .as_deref()
                    .unwrap_or("one that did not report its address")
            )
        })
    }

    /// The step to run after the `VerifyEquals` at `index` read back `back`:
    /// the next one on a match; the start of the filesystem steps on the
    /// first mismatch (the one retry, `retried` records it); `None` — fail —
    /// on a second mismatch, or on a mismatch outside the filesystem steps.
    /// Shared by every executor so the retry rule has one home (the browser
    /// asks it across the JS boundary).
    pub fn after_verify(&self, index: usize, back: &[u8], retried: &mut bool) -> Option<usize> {
        let Some(FlashStep::VerifyEquals { bytes, .. }) = self.steps.get(index) else {
            return None;
        };
        if back == bytes.as_slice() {
            return Some(index + 1);
        }
        if *retried || self.lpfs_start > index {
            return None;
        }
        *retried = true;
        Some(self.lpfs_start)
    }
}

/// A step as an executor that cannot see the expected bytes receives it —
/// the browser's JS, which writes and reads back but never compares (Rust
/// answers [`FlashPlan::after_verify`]). A verify carries only its length.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutorStep<'a> {
    Firmware,
    Erase { offset: u32, length: u32 },
    Write { offset: u32, bytes: &'a [u8] },
    Verify { offset: u32, length: u32 },
}

impl FlashPlan {
    /// The steps in executor shape, in order.
    pub fn executor_steps(&self) -> Vec<ExecutorStep<'_>> {
        self.steps
            .iter()
            .map(|step| match step {
                FlashStep::WriteFirmware => ExecutorStep::Firmware,
                FlashStep::Erase { offset, length } => ExecutorStep::Erase {
                    offset: *offset,
                    length: *length,
                },
                FlashStep::Write { offset, bytes } => ExecutorStep::Write {
                    offset: *offset,
                    bytes,
                },
                FlashStep::VerifyEquals { offset, bytes } => ExecutorStep::Verify {
                    offset: *offset,
                    length: bytes.len() as u32,
                },
            })
            .collect()
    }
}

/// What an executor does for each kind of step — the synchronous flashers
/// (espflash on the host, the fake board's flash map) implement it, and
/// [`run_plan`] owns the order, the readback and the one retry.
pub trait FlashStepTarget {
    type Error: core::fmt::Display;
    fn write_firmware(&mut self) -> Result<(), Self::Error>;
    fn erase(&mut self, offset: u32, length: u32) -> Result<(), Self::Error>;
    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error>;
    fn read(&mut self, offset: u32, length: u32) -> Result<Vec<u8>, Self::Error>;
}

/// Why a plan did not complete.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanError {
    /// `requires_backup` without `backup_confirmed`: nothing was written.
    BackupNotConfirmed,
    /// A step failed; `index` is its position in the plan.
    Step {
        index: usize,
        label: String,
        error: String,
    },
    /// A readback did not match even after the filesystem steps were re-run
    /// once.
    VerifyMismatch { offset: u32 },
}

impl core::fmt::Display for PlanError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BackupNotConfirmed => f.write_str(
                "refusing to rewrite the board's files: their backup was not confirmed stored",
            ),
            Self::Step {
                index,
                label,
                error,
            } => write!(f, "step {} ({label}) failed: {error}", index + 1),
            Self::VerifyMismatch { offset } => write!(
                f,
                "the files written at {offset:#x} did not read back the same, twice"
            ),
        }
    }
}

/// Run `plan` against `target`: each step in order; a `VerifyEquals` that
/// does not match re-runs the filesystem tail once, then fails.
/// `on_step(index, step)` fires before each step (progress).
pub fn run_plan<T: FlashStepTarget>(
    target: &mut T,
    plan: &FlashPlan,
    mut on_step: impl FnMut(usize, &FlashStep),
) -> Result<(), PlanError> {
    if !plan.may_execute() {
        return Err(PlanError::BackupNotConfirmed);
    }
    let mut retried = false;
    let mut index = 0;
    while index < plan.steps.len() {
        let step = &plan.steps[index];
        on_step(index, step);
        let failed = |error: T::Error| PlanError::Step {
            index,
            label: step.label().to_string(),
            error: error.to_string(),
        };
        match step {
            FlashStep::WriteFirmware => target.write_firmware().map_err(failed)?,
            FlashStep::Erase { offset, length } => {
                target.erase(*offset, *length).map_err(failed)?
            }
            FlashStep::Write { offset, bytes } => target.write(*offset, bytes).map_err(failed)?,
            FlashStep::VerifyEquals { offset, bytes } => {
                let back = target.read(*offset, bytes.len() as u32).map_err(failed)?;
                match plan.after_verify(index, &back, &mut retried) {
                    Some(next) => {
                        index = next;
                        continue;
                    }
                    None => return Err(PlanError::VerifyMismatch { offset: *offset }),
                }
            }
        }
        index += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An in-memory flash whose `read` can be told to corrupt its first N
    /// readbacks.
    struct Mem {
        flash: Vec<u8>,
        bad_reads: u32,
        writes: Vec<u32>,
    }

    impl FlashStepTarget for Mem {
        type Error = String;
        fn write_firmware(&mut self) -> Result<(), String> {
            self.writes.push(0);
            Ok(())
        }
        fn erase(&mut self, offset: u32, length: u32) -> Result<(), String> {
            self.flash[offset as usize..(offset + length) as usize].fill(0xFF);
            Ok(())
        }
        fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), String> {
            self.writes.push(offset);
            self.flash[offset as usize..offset as usize + bytes.len()].copy_from_slice(bytes);
            Ok(())
        }
        fn read(&mut self, offset: u32, length: u32) -> Result<Vec<u8>, String> {
            let mut out = self.flash[offset as usize..(offset + length) as usize].to_vec();
            if self.bad_reads > 0 {
                self.bad_reads -= 1;
                out[0] ^= 1;
            }
            Ok(out)
        }
    }

    fn plan() -> FlashPlan {
        FlashPlan {
            steps: vec![
                FlashStep::WriteFirmware,
                FlashStep::Erase {
                    offset: 0x1000,
                    length: 0x1000,
                },
                FlashStep::Write {
                    offset: 0x1000,
                    bytes: vec![7; 0x1000],
                },
                FlashStep::VerifyEquals {
                    offset: 0x1000,
                    bytes: vec![7; 0x1000],
                },
            ],
            requires_backup: true,
            backup_confirmed: true,
            lpfs_start: 1,
            base_mac: None,
        }
    }

    fn mem(bad_reads: u32) -> Mem {
        Mem {
            flash: vec![0xFF; 0x4000],
            bad_reads,
            writes: Vec::new(),
        }
    }

    #[test]
    fn a_clean_plan_runs_every_step_once() {
        let mut target = mem(0);
        run_plan(&mut target, &plan(), |_, _| {}).unwrap();
        assert_eq!(target.writes, vec![0, 0x1000]);
    }

    #[test]
    fn one_bad_readback_re_runs_the_filesystem_steps_and_not_the_firmware() {
        let mut target = mem(1);
        run_plan(&mut target, &plan(), |_, _| {}).unwrap();
        assert_eq!(target.writes, vec![0, 0x1000, 0x1000]);
    }

    #[test]
    fn two_bad_readbacks_fail_naming_the_region() {
        let mut target = mem(2);
        assert_eq!(
            run_plan(&mut target, &plan(), |_, _| {}),
            Err(PlanError::VerifyMismatch { offset: 0x1000 })
        );
    }

    #[test]
    fn the_executor_shape_hides_the_expected_bytes_of_a_verify() {
        let plan = plan();
        let steps = plan.executor_steps();
        assert_eq!(steps[0], ExecutorStep::Firmware);
        assert_eq!(
            steps[1],
            ExecutorStep::Erase {
                offset: 0x1000,
                length: 0x1000
            }
        );
        assert!(
            matches!(steps[2], ExecutorStep::Write { offset: 0x1000, bytes } if bytes.len() == 0x1000)
        );
        assert_eq!(
            steps[3],
            ExecutorStep::Verify {
                offset: 0x1000,
                length: 0x1000
            }
        );
    }

    #[test]
    fn a_plan_for_one_board_refuses_another_and_an_anonymous_one() {
        let mut plan = plan();
        assert_eq!(plan.refuse_board(None), None, "no board named: any board");
        plan.base_mac = Some("10:bd:a3:b0:8e:30".to_string());
        assert_eq!(plan.refuse_board(Some("10:BD:A3:B0:8E:30")), None);
        assert!(plan.refuse_board(Some("aa:bb:cc:dd:ee:ff")).is_some());
        assert!(plan.refuse_board(None).is_some());
    }

    #[test]
    fn after_verify_is_the_retry_rule() {
        let plan = plan();
        let good = vec![7u8; 0x1000];
        let bad = vec![8u8; 0x1000];
        let mut retried = false;
        assert_eq!(plan.after_verify(3, &good, &mut retried), Some(4));
        assert_eq!(plan.after_verify(3, &bad, &mut retried), Some(1));
        assert!(retried);
        assert_eq!(plan.after_verify(3, &bad, &mut retried), None);
        assert_eq!(
            plan.after_verify(0, &good, &mut false),
            None,
            "not a verify step"
        );
    }

    #[test]
    fn an_unconfirmed_backup_writes_nothing() {
        let mut target = mem(0);
        let mut plan = plan();
        plan.backup_confirmed = false;
        assert_eq!(
            run_plan(&mut target, &plan, |_, _| {}),
            Err(PlanError::BackupNotConfirmed)
        );
        assert!(target.writes.is_empty());
    }

    #[test]
    fn a_plan_that_needs_a_backup_waits_for_its_confirmation() {
        let mut plan = FlashPlan {
            steps: vec![FlashStep::WriteFirmware],
            requires_backup: true,
            lpfs_start: 1,
            ..FlashPlan::default()
        };
        assert!(!plan.may_execute());
        plan.backup_confirmed = true;
        assert!(plan.may_execute());
        assert!(plan.lpfs_tail().is_empty());
    }
}
