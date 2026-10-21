//! `--flash-cut <spec>`: a power cut at the Nth flash command in a chip's
//! `lpfs` partition, as one flag — the spec and its parser, shared by every
//! chip that arms a cut (the C6 since tree-store M4; the S3 since S3 radio M7
//! P5). Moved here from `lp-emu-esp32c6`, which re-exports it unchanged.
//!
//! ```text
//! <n>:<model>:<seed>[,range=<off>+<len>][,then=stop|power-cycle]
//! ```
//!
//! - `<n>`: the 0-based index of the in-range program or erase command to
//!   cut, counted from power-on (`0` cuts the first one). Reads, WREN and
//!   status polls are not counted ([`super::flash_cut`]).
//! - `<model>`: a `lp-nor-sim` tear model by name (`TearModel::NAMED`):
//!   `clean`, `byte_prefix`, `random_bits`, `calibrated`,
//!   `calibrated_zeroing`, `calibrated_zeroing_mid_word`,
//!   `calibrated_all_zero`, `calibrated_first_lift`, `calibrated_erasing`,
//!   `calibrated_reads_ff_weak`, `calibrated_reads_ff`,
//!   `calibrated_before_erase`, `calibrated_mid_word`.
//! - `<seed>`: every random choice of the tear and of the weak reads after
//!   it. Decimal.
//! - `range=<off>+<len>`: the flash bytes to count in, hex (`0x…`) or
//!   decimal. Left out, the chip's own `lpfs` row — which only the chip
//!   crate can read, so it hands that range to [`FlashCutSpec::plan_in`].
//! - `then=`: what the machine does when the cut fires. `stop` (the
//!   default) ends the run with the machine's `Outcome::PowerCut`;
//!   `power-cycle` restores the supply — the machine back to power-on, the
//!   flash as the cut left it — and runs on, the plan spent.
//!
//! Options may be joined with `;` as well as `,`, so the spec fits inside a
//! comma-separated list (`emu serve`'s `flash_cut=` board option).

use core::ops::Range;

use super::flash_cut::{FlashCut, TearModel};

/// What a run with a flash-cut plan carries in its configuration label,
/// after the seam atoms and before the pace: `lp-emu:esp32c6:t1+flash-cut`,
/// `lp-emu:esp32s3:t1+flash-cut`.
/// `lp-emu-validate` reads the same spelling and refuses to record or run
/// such a configuration; `lp-cli` owns the test that the two agree.
pub const FLASH_CUT_MARKER: &str = "+flash-cut";

/// What the machine does when the cut fires.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AfterCut {
    /// End the run with `Outcome::PowerCut`.
    #[default]
    Stop,
    /// Power-cycle the board and run on.
    PowerCycle,
}

impl AfterCut {
    pub const fn as_str(self) -> &'static str {
        match self {
            AfterCut::Stop => "stop",
            AfterCut::PowerCycle => "power-cycle",
        }
    }
}

/// A parsed `--flash-cut`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlashCutSpec {
    pub at: u64,
    pub tear: TearModel,
    pub seed: u64,
    /// `None`: the chip's `lpfs` row, resolved by the chip crate at build.
    pub range: Option<Range<u32>>,
    pub then: AfterCut,
}

impl FlashCutSpec {
    /// A cut at `at` under `tear`, in the default range, stopping the run.
    pub fn new(at: u64, tear: TearModel, seed: u64) -> Self {
        Self {
            at,
            tear,
            seed,
            range: None,
            then: AfterCut::Stop,
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut parts = text.split([',', ';']).map(str::trim);
        let head = parts.next().unwrap_or_default();
        let fields: Vec<&str> = head.split(':').collect();
        let [at, model, seed] = fields.as_slice() else {
            return Err(format!(
                "`{text}`: expected <n>:<model>:<seed>[,range=<off>+<len>][,then=stop|power-cycle]"
            ));
        };
        let at = at
            .parse::<u64>()
            .map_err(|_| format!("`{text}`: the op index `{at}` is not a whole number"))?;
        let tear = TearModel::from_name(model).ok_or_else(|| {
            format!(
                "`{text}`: no tear model `{model}` (known: {})",
                TearModel::NAMED
                    .iter()
                    .map(|t| t.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        let seed = seed
            .parse::<u64>()
            .map_err(|_| format!("`{text}`: the seed `{seed}` is not a whole number"))?;
        let mut spec = Self::new(at, tear, seed);
        for option in parts {
            match option.split_once('=') {
                Some(("range", value)) => {
                    let (off, len) = value
                        .split_once('+')
                        .ok_or_else(|| format!("`{text}`: range `{value}` is not <off>+<len>"))?;
                    let (off, len) = (number(off, text)?, number(len, text)?);
                    if len == 0 {
                        return Err(format!("`{text}`: an empty range counts nothing"));
                    }
                    let end = off
                        .checked_add(len)
                        .ok_or_else(|| format!("`{text}`: range `{value}` leaves the 4 GiB map"))?;
                    spec.range = Some(off..end);
                }
                Some(("then", "stop")) => spec.then = AfterCut::Stop,
                Some(("then", "power-cycle")) => spec.then = AfterCut::PowerCycle,
                Some(("then", other)) => {
                    return Err(format!(
                        "`{text}`: then=`{other}`: expected stop or power-cycle"
                    ));
                }
                _ => {
                    return Err(format!(
                        "`{text}`: unknown option `{option}` (range=<off>+<len>, \
                         then=stop|power-cycle)"
                    ));
                }
            }
        }
        Ok(spec)
    }

    /// The plan, over the spec's own `range` or, when it named none,
    /// `default_range` — the chip's `lpfs` row, which only the chip crate can
    /// read off its partition table.
    pub fn plan_in(&self, default_range: Range<u32>) -> FlashCut {
        FlashCut {
            range: self.range.clone().unwrap_or(default_range),
            at: self.at,
            tear: self.tear,
            seed: self.seed,
        }
    }
}

impl core::fmt::Display for FlashCutSpec {
    /// The spec as `parse` reads it (a range, when given, in hex).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}:{}:{}", self.at, self.tear.name(), self.seed)?;
        if let Some(range) = &self.range {
            write!(
                f,
                ",range={:#x}+{:#x}",
                range.start,
                range.end - range.start
            )?;
        }
        if self.then != AfterCut::Stop {
            write!(f, ",then={}", self.then.as_str())?;
        }
        Ok(())
    }
}

fn number(text: &str, spec: &str) -> Result<u32, String> {
    let text = text.trim();
    let parsed = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => text.parse(),
    };
    parsed.map_err(|_| format!("`{spec}`: `{text}` is not a number (hex 0x… or decimal)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spec_parses_with_its_defaults_and_its_options() {
        let spec = FlashCutSpec::parse("12:calibrated:7").unwrap();
        assert_eq!(spec, FlashCutSpec::new(12, TearModel::Calibrated, 7));
        let spec =
            FlashCutSpec::parse("0:calibrated_all_zero:3,range=0x370000+0x80000,then=power-cycle")
                .unwrap();
        assert_eq!(spec.at, 0);
        assert_eq!(spec.tear, TearModel::CalibratedAllZero);
        assert_eq!(spec.range, Some(0x37_0000..0x3F_0000));
        assert_eq!(spec.then, AfterCut::PowerCycle);
        // `;` joins options too, for a list that already uses commas.
        assert_eq!(
            FlashCutSpec::parse("0:calibrated_all_zero:3;range=0x370000+0x80000;then=power-cycle")
                .unwrap(),
            spec
        );
        assert_eq!(
            spec.to_string(),
            "0:calibrated_all_zero:3,range=0x370000+0x80000,then=power-cycle"
        );
        assert_eq!(FlashCutSpec::parse(&spec.to_string()).unwrap(), spec);
        for t in TearModel::NAMED {
            let text = format!("1:{}:2", t.name());
            assert_eq!(FlashCutSpec::parse(&text).unwrap().tear, t, "{text}");
        }
    }

    #[test]
    fn a_bad_spec_says_what_is_wrong() {
        for (text, says) in [
            ("12:calibrated", "expected <n>:<model>:<seed>"),
            ("x:calibrated:1", "op index"),
            ("1:gentle:1", "no tear model `gentle`"),
            ("1:clean:-1", "seed"),
            ("1:clean:1,range=0x1000", "<off>+<len>"),
            ("1:clean:1,range=0x1000+0", "empty range"),
            ("1:clean:1,then=reboot", "stop or power-cycle"),
            ("1:clean:1,speed=fast", "unknown option"),
        ] {
            let err = FlashCutSpec::parse(text).unwrap_err();
            assert!(err.contains(says), "{text}: {err}");
        }
    }

    #[test]
    fn a_plan_takes_the_specs_range_or_the_chips_default() {
        let spec = FlashCutSpec::new(3, TearModel::Clean, 1);
        assert_eq!(spec.plan_in(0x10..0x20).range, 0x10..0x20);
        let spec = FlashCutSpec::parse("3:clean:1,range=0x100+0x10").unwrap();
        let plan = spec.plan_in(0x10..0x20);
        assert_eq!(plan.range, 0x100..0x110);
        assert_eq!((plan.at, plan.tear, plan.seed), (3, TearModel::Clean, 1));
    }
}
