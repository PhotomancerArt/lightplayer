//! The seam implementations this emulator has, and what each one answers.
//!
//! An implementation is one way of answering one declared seam: `led=fast`
//! answers `ws281x_wait_step`. Its label atom is `<label>=<implementation>`
//! (the configuration label's spelling, `lp-emu:esp32c6:t2+led=fast`); the
//! implementation name `real` is reserved for "not engaged" and never
//! appears here. The two test seams share the label `test` (`test=echo`,
//! `test=take`), so a request is refused when it names one **declared seam**
//! twice, not one label twice.
//!
//! The test implementations (`test=echo`, `test=take`) exist only with the
//! dev feature `test-seams`, which no shipped command line turns on.

use lp_seam::SeamKind;

/// How a chip machine answers an engaged seam's call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeamAnswer {
    /// Return to the caller, then park the hart until an interrupt it would
    /// wake for (`wfi`'s wake condition). `led=fast`.
    ParkUntilInterrupt,
    /// Read `a0..a2`, write a value that is not the silicon answer to `a0`.
    /// `test=echo`.
    TestEcho,
    /// Copy what the endpoint holds into the buffer the call handed over
    /// (`a0` endpoint, `a1` buffer, `a2` capacity), return the count.
    /// `test=take`.
    Take,
}

/// One emulator implementation of one seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeamImpl {
    /// The label's seam name (`led`).
    pub label: &'static str,
    /// The implementation name (`fast`).
    pub implementation: &'static str,
    /// The `lp_seam` declaration it answers.
    pub decl_id: u16,
    pub kind: SeamKind,
    /// The per-call trace verb (`wait-step`).
    pub verb: &'static str,
    pub answer: SeamAnswer,
}

impl SeamImpl {
    /// `led=fast`.
    pub fn atom(&self) -> String {
        format!("{}={}", self.label, self.implementation)
    }

    pub fn decl(&self) -> &'static lp_seam::SeamDecl {
        lp_seam::SeamDecl::by_id(self.decl_id).expect("an implementation answers a declared seam")
    }

    /// Whether this is a test implementation (`lp_seam::test_seams`).
    pub fn is_test(&self) -> bool {
        lp_seam::test_seams::is_test_id(self.decl_id)
    }
}

/// `led=fast`: the LED performance seam (L1).
pub const LED_FAST: SeamImpl = SeamImpl {
    label: "led",
    implementation: "fast",
    decl_id: lp_seam::ws281x_wait_step::ID,
    kind: SeamKind::Performance,
    verb: "wait-step",
    answer: SeamAnswer::ParkUntilInterrupt,
};

/// `test=echo` (feature `test-seams`).
#[cfg(feature = "test-seams")]
pub const TEST_ECHO: SeamImpl = SeamImpl {
    label: "test",
    implementation: "echo",
    decl_id: lp_seam::test_echo::ID,
    kind: SeamKind::Performance,
    verb: "echo",
    answer: SeamAnswer::TestEcho,
};

/// `test=take` (feature `test-seams`).
#[cfg(feature = "test-seams")]
pub const TEST_TAKE: SeamImpl = SeamImpl {
    label: "test",
    implementation: "take",
    decl_id: lp_seam::test_take::ID,
    kind: SeamKind::Capability,
    verb: "take",
    answer: SeamAnswer::Take,
};

/// Every implementation this emulator has, in label order.
pub fn implementations() -> &'static [SeamImpl] {
    #[cfg(not(feature = "test-seams"))]
    {
        &[LED_FAST]
    }
    #[cfg(feature = "test-seams")]
    {
        &[LED_FAST, TEST_ECHO, TEST_TAKE]
    }
}

/// The implementation named `<label>=<implementation>`, if this emulator has
/// it.
pub fn find(label: &str, implementation: &str) -> Option<&'static SeamImpl> {
    implementations()
        .iter()
        .find(|i| i.label == label && i.implementation == implementation)
}

/// The seams on in every emulated run: the capability implementations, never
/// a performance one and never a test one. Empty today — no capability seam
/// ships yet — so a default run scans nothing.
pub fn capability_defaults() -> impl Iterator<Item = &'static SeamImpl> {
    implementations()
        .iter()
        .filter(|i| i.kind == SeamKind::Capability && !i.is_test())
}

/// `led=fast, …`: what this emulator can engage, for an error message.
pub fn known_atoms() -> String {
    implementations()
        .iter()
        .map(SeamImpl::atom)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn led_fast_is_a_performance_seam_on_the_wait_step() {
        let led = find("led", "fast").unwrap();
        assert_eq!(led.kind, SeamKind::Performance);
        assert_eq!(led.decl().symbol, "lp_seam_ws281x_wait_step");
        assert_eq!(led.atom(), "led=fast");
        assert!(!led.is_test());
    }

    #[test]
    fn no_performance_or_test_seam_is_a_default() {
        assert_eq!(capability_defaults().count(), 0, "none ship yet");
        for i in capability_defaults() {
            assert_eq!(i.kind, SeamKind::Capability);
            assert!(!i.is_test());
        }
    }

    #[test]
    fn every_implementation_answers_a_declared_seam_of_its_kind() {
        for i in implementations() {
            assert_eq!(i.decl().kind, i.kind, "{}", i.atom());
            assert_ne!(i.implementation, "real");
        }
    }

    #[cfg(feature = "test-seams")]
    #[test]
    fn the_test_implementations_exist_only_under_the_feature() {
        assert!(find("test", "echo").unwrap().is_test());
        assert!(find("test", "take").unwrap().is_test());
    }

    #[cfg(not(feature = "test-seams"))]
    #[test]
    fn a_shipped_build_has_no_test_implementation() {
        assert!(implementations().iter().all(|i| !i.is_test()));
    }
}
