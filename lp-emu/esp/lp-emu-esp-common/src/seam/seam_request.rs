//! What a run asked for — `--seams led=fast`, `--seams-prefer led=fast` —
//! and the configuration label it earns.
//!
//! Spelled per the roadmap's M9 pick (option B): atoms `<seam>=<impl>`,
//! joined with `+` (or a space, because a browser decodes `+` as one),
//! sorted; `none` asks for nothing; `real` is the reserved "not engaged"
//! implementation and never reaches a label.
//!
//! Two strengths (the plan's FD5):
//!
//! - **strict** (`--seams`): a seam that cannot engage is a hard error. Asking
//!   for a fast run and silently getting a slow one wastes a measurement.
//! - **soft** (`--seams-prefer`): engage what the image allows, else one loud
//!   line and run with none. Studio's boards ask softly, because a saved
//!   board can hold firmware older than the emulator serving it.
//!
//! The **capability defaults** ([`super::seam_impl::capability_defaults`])
//! are added softly unless the request says `none`, or pins that seam to
//! `real`. They are empty today, so a default request is empty, and an empty
//! request scans nothing.

use std::fmt;

use super::seam_impl::{self, SeamImpl};

/// How hard a seam was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strength {
    /// A seam that cannot engage stops the run.
    Strict,
    /// A seam that cannot engage is one line, then the run goes on without.
    Soft,
}

/// The seams a run asked to engage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeamRequest {
    /// Asked for by name, in atom order.
    asked: Vec<(&'static SeamImpl, Strength)>,
    /// Seam labels pinned to `real` (`net=real`): never engaged, defaults
    /// included.
    pinned_real: Vec<String>,
    /// Whether the capability defaults are added (not after `none`).
    defaults: bool,
}

impl Default for SeamRequest {
    /// The capability defaults, softly, and nothing else.
    fn default() -> Self {
        Self {
            asked: Vec::new(),
            pinned_real: Vec::new(),
            defaults: true,
        }
    }
}

impl SeamRequest {
    /// Nothing at all, not even the capability defaults.
    pub fn none() -> Self {
        Self {
            defaults: false,
            ..Self::default()
        }
    }

    /// `--seams <text>`: `none`, or strict atoms.
    pub fn strict(text: &str) -> Result<Self, String> {
        Self::default().with(text, Strength::Strict)
    }

    /// `--seams-prefer <text>`: soft atoms.
    pub fn prefer(text: &str) -> Result<Self, String> {
        Self::default().with(text, Strength::Soft)
    }

    /// Add `text`'s atoms at `strength`. `none` turns everything off,
    /// defaults included; an empty text adds nothing.
    pub fn with(mut self, text: &str, strength: Strength) -> Result<Self, String> {
        let text = text.trim();
        if text == "none" {
            return Ok(Self::none());
        }
        for atom in text.split(['+', ' ']).filter(|a| !a.is_empty()) {
            let (label, implementation) = atom
                .split_once('=')
                .ok_or_else(|| format!("seam atom `{atom}`: expected <seam>=<impl>"))?;
            if self.pinned_real.iter().any(|l| l == label) {
                return Err(format!("seam `{label}` named twice"));
            }
            if implementation == "real" {
                if self.asked.iter().any(|(i, _)| i.label == label) {
                    return Err(format!("seam `{label}` named twice"));
                }
                self.pinned_real.push(label.to_string());
                continue;
            }
            let found = seam_impl::find(label, implementation).ok_or_else(|| {
                format!(
                    "no seam implementation `{atom}` (this emulator has: {})",
                    seam_impl::known_atoms()
                )
            })?;
            if self.asked.iter().any(|(i, _)| i.decl_id == found.decl_id) {
                return Err(format!("seam `{atom}` named twice"));
            }
            self.asked.push((found, strength));
        }
        self.asked.sort_by_key(|(i, _)| i.atom());
        Ok(self)
    }

    /// Every seam this request wants engaged, with its strength: what was
    /// asked for, then the capability defaults not already named, in atom
    /// order.
    pub fn wanted(&self) -> Vec<(&'static SeamImpl, Strength)> {
        let mut out = self.asked.clone();
        if self.defaults {
            for d in seam_impl::capability_defaults() {
                let named = out.iter().any(|(i, _)| i.decl_id == d.decl_id)
                    || self.pinned_real.iter().any(|l| l == d.label);
                if !named {
                    out.push((d, Strength::Soft));
                }
            }
        }
        out.sort_by_key(|(i, _)| i.atom());
        out
    }

    /// Nothing to engage: the machine never scans.
    pub fn is_empty(&self) -> bool {
        self.wanted().is_empty()
    }

    /// Whether any wanted seam is strict.
    pub fn has_strict(&self) -> bool {
        self.wanted().iter().any(|(_, s)| *s == Strength::Strict)
    }
}

/// `base` plus one `+<seam>=<impl>` per **engaged** seam, sorted. With none
/// engaged, exactly `base`.
pub fn label(base: &str, engaged: &[&'static SeamImpl]) -> String {
    let mut atoms: Vec<String> = engaged.iter().map(|i| i.atom()).collect();
    atoms.sort();
    let mut out = base.to_string();
    for a in atoms {
        out.push('+');
        out.push_str(&a);
    }
    out
}

impl fmt::Display for SeamRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let wanted = self.wanted();
        if wanted.is_empty() {
            return f.write_str("none");
        }
        let atoms = |s: Strength| {
            wanted
                .iter()
                .filter(|(_, x)| *x == s)
                .map(|(i, _)| i.atom())
                .collect::<Vec<_>>()
                .join("+")
        };
        let (strict, soft) = (atoms(Strength::Strict), atoms(Strength::Soft));
        match (strict.is_empty(), soft.is_empty()) {
            (false, true) => f.write_str(&strict),
            (true, false) => write!(f, "prefer {soft}"),
            _ => write!(f, "{strict}, prefer {soft}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_empty_and_real_engage_nothing_and_leave_the_label_alone() {
        for text in ["", "none", "led=real", " none "] {
            let r = SeamRequest::strict(text).unwrap();
            assert!(r.is_empty(), "{text}");
            assert_eq!(label("lp-emu:esp32c6:t2", &[]), "lp-emu:esp32c6:t2");
            assert_eq!(r.to_string(), "none");
        }
        assert!(
            SeamRequest::default().is_empty(),
            "no capability default yet"
        );
        assert!(SeamRequest::none().is_empty());
    }

    #[test]
    fn led_fast_is_strict_or_soft_and_labelled_with_a_plus_atom() {
        let r = SeamRequest::strict("led=fast").unwrap();
        assert_eq!(r.wanted().len(), 1);
        assert_eq!(r.wanted()[0].1, Strength::Strict);
        assert!(r.has_strict());
        assert_eq!(r.to_string(), "led=fast");
        let p = SeamRequest::prefer("led=fast").unwrap();
        assert_eq!(p.wanted()[0].1, Strength::Soft);
        assert!(!p.has_strict());
        assert_eq!(p.to_string(), "prefer led=fast");
        let led = p.wanted()[0].0;
        assert_eq!(
            label("lp-emu:esp32c6:t2", &[led]),
            "lp-emu:esp32c6:t2+led=fast"
        );
    }

    #[test]
    fn a_space_joins_atoms_like_a_plus() {
        // A browser hands `?seams=led=fast+x=y` over as `led=fast x=y`.
        assert_eq!(
            SeamRequest::strict("led=fast ").unwrap(),
            SeamRequest::strict("led=fast").unwrap()
        );
        assert!(SeamRequest::strict("led=fast nope=x").is_err());
    }

    #[test]
    fn unknown_malformed_or_doubled_seams_are_refused() {
        assert!(
            SeamRequest::strict("led=slow")
                .unwrap_err()
                .contains("led=fast")
        );
        assert!(SeamRequest::strict("led").is_err());
        assert!(SeamRequest::strict("led=fast+led=fast").is_err());
        assert!(SeamRequest::strict("led=fast+led=real").is_err());
        assert!(
            SeamRequest::strict("led=fast")
                .unwrap()
                .with("led=fast", Strength::Soft)
                .is_err(),
            "strict and soft for one seam"
        );
    }

    #[test]
    fn labels_sort_their_atoms() {
        let led = seam_impl::find("led", "fast").unwrap();
        let fake = SeamImpl {
            label: "aaa",
            implementation: "x",
            ..*led
        };
        let fake: &'static SeamImpl = Box::leak(Box::new(fake));
        assert_eq!(label("b", &[led, fake]), "b+aaa=x+led=fast");
    }

    #[cfg(feature = "test-seams")]
    #[test]
    fn the_two_test_seams_share_a_label_but_not_a_seam() {
        let r = SeamRequest::strict("test=take+test=echo").unwrap();
        let atoms: Vec<String> = r.wanted().iter().map(|(i, _)| i.atom()).collect();
        assert_eq!(atoms, ["test=echo", "test=take"]);
        assert!(
            SeamRequest::strict("test=take").unwrap().wanted()[0]
                .0
                .is_test()
        );
    }
}
