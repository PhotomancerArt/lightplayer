//! What a run asked for: `--seams led=fast`, and the label it earns.
//!
//! Spelled per M9's recommendation (option B): atoms `<seam>=<impl>`, joined
//! with `+`, sorted by seam name; `none` engages nothing; `real` is the
//! reserved "not engaged" implementation and never appears in a label.
//! Provisional until G0.

use std::fmt;

/// An emulator implementation of one seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeamImpl {
    /// The label's seam name (`led`).
    pub label: &'static str,
    /// The implementation name (`fast`).
    pub implementation: &'static str,
    /// The `lp_seam` declaration it answers.
    pub decl_id: u16,
    /// The per-call trace verb (`wait-step`).
    pub verb: &'static str,
}

/// Every implementation this emulator has. One today.
pub const IMPLEMENTATIONS: &[SeamImpl] = &[
    SeamImpl {
        label: "led",
        implementation: "fast",
        decl_id: lp_seam::ws281x_wait_step::ID,
        verb: "wait-step",
    },
    // SPIKE ONLY: the wake probe (M0 part B), against a
    // `spike_seam_wake_probe` firmware.
    SeamImpl {
        label: "probe",
        implementation: "host",
        decl_id: lp_seam::probe_take::ID,
        verb: "take",
    },
];

/// The seams a run asked to engage, sorted by label.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeamRequest {
    pub engaged: Vec<SeamImpl>,
    /// `auto`: engage the seams that are on by default (capability seams —
    /// none exist yet), and when the image's table cannot be read, say so in
    /// one loud line and engage nothing (PD5's default half). An explicit
    /// request that cannot engage is a hard error instead.
    pub auto: bool,
}

impl SeamRequest {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.engaged.is_empty()
    }

    /// Parse `none`, or `led=fast`, or `led=fast+net=lan` (a space works as
    /// the joiner too, because a browser decodes `+` as a space).
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() || text == "none" {
            return Ok(Self::none());
        }
        if text == "auto" {
            return Ok(Self {
                engaged: Vec::new(),
                auto: true,
            });
        }
        let mut engaged: Vec<SeamImpl> = Vec::new();
        for atom in text.split(['+', ' ']).filter(|a| !a.is_empty()) {
            let (label, implementation) = atom
                .split_once('=')
                .ok_or_else(|| format!("seam atom `{atom}`: expected <seam>=<impl>"))?;
            if engaged.iter().any(|s| s.label == label) {
                return Err(format!("seam `{label}` named twice"));
            }
            if implementation == "real" {
                continue;
            }
            let found = IMPLEMENTATIONS
                .iter()
                .find(|i| i.label == label && i.implementation == implementation)
                .ok_or_else(|| {
                    format!(
                        "no seam implementation `{atom}` (this emulator has: {})",
                        IMPLEMENTATIONS
                            .iter()
                            .map(|i| format!("{}={}", i.label, i.implementation))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })?;
            engaged.push(*found);
        }
        engaged.sort_by_key(|s| s.label);
        Ok(Self {
            engaged,
            auto: false,
        })
    }

    /// `base` plus one `+<seam>=<impl>` per engaged seam. With none engaged,
    /// exactly `base`.
    pub fn label(&self, base: &str) -> String {
        let mut out = base.to_string();
        for s in &self.engaged {
            out.push('+');
            out.push_str(s.label);
            out.push('=');
            out.push_str(s.implementation);
        }
        out
    }
}

impl fmt::Display for SeamRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.engaged.is_empty() {
            return f.write_str("none");
        }
        let atoms: Vec<String> = self
            .engaged
            .iter()
            .map(|s| format!("{}={}", s.label, s.implementation))
            .collect();
        f.write_str(&atoms.join("+"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_and_real_engage_nothing_and_leave_the_label_alone() {
        for text in ["", "none", "led=real"] {
            let r = SeamRequest::parse(text).unwrap();
            assert!(r.is_empty(), "{text}");
            assert_eq!(r.label("lp-emu:esp32c6:t2"), "lp-emu:esp32c6:t2");
        }
    }

    #[test]
    fn led_fast_is_labelled_with_a_plus_atom() {
        let r = SeamRequest::parse("led=fast").unwrap();
        assert_eq!(r.label("lp-emu:esp32c6:t2"), "lp-emu:esp32c6:t2+led=fast");
        assert_eq!(r.to_string(), "led=fast");
        assert_eq!(SeamRequest::parse("led=fast ").unwrap(), r);
    }

    #[test]
    fn unknown_or_doubled_seams_are_refused() {
        assert!(SeamRequest::parse("led=slow").is_err());
        assert!(SeamRequest::parse("led").is_err());
        assert!(SeamRequest::parse("led=fast+led=fast").is_err());
    }
}
