//! The seam implementations this emulator has, and what each one answers.
//!
//! An implementation is one way of answering a declared seam: `led=fast`
//! answers `ws281x_wait_step`, and `net=lan` answers all nine calls of the
//! network seam (`lp_seam::net::CALLS`) — one atom arms every one of their
//! sites, so a run never engages half a network. Its label atom is
//! `<label>=<implementation>` (the configuration label's spelling,
//! `lp-emu:esp32c6:t2+led=fast`); the implementation name `real` is reserved
//! for "not engaged" and never appears here. The two test seams share the
//! label `test` (`test=echo`, `test=take`), so a request is refused when it
//! names one **declared seam** twice, not one label twice.
//!
//! The test implementations (`test=echo`, `test=take`) exist only with the
//! dev feature `test-seams`, which no shipped command line turns on.
//!
//! # What an answer costs
//!
//! **Zero guest cycles, for every call of every implementation.** An answer
//! is `pc = ra` plus whatever its implementation does to registers and to the
//! buffers the call handed over, and no cycle is charged for it. Unmeasured,
//! and stated here once (the seams roadmap's D2): a silicon figure for a seam
//! call means nothing, because on silicon the call runs the real body.

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
    /// One of the network seam's calls, told apart by the site's declaration
    /// (`lp_seam::net`), answered from the board's endpoint and its virtual
    /// LAN (`super::net`). `net=lan`.
    Net,
}

/// One emulator implementation of one seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeamImpl {
    /// The label's seam name (`led`).
    pub label: &'static str,
    /// The implementation name (`fast`).
    pub implementation: &'static str,
    /// The `lp_seam` declarations it answers: one for most, every call of a
    /// multi-call seam (`net=lan`). The first is the **primary**: it names
    /// the seam in messages, and on a switch-shape seam it is the entry that
    /// carries the engaged byte.
    pub decls: &'static [lp_seam::SeamDecl],
    pub kind: SeamKind,
    /// The per-call trace verb (`wait-step`) of a one-call implementation; a
    /// multi-call one names each call after its declaration
    /// ([`Self::verb_of`]).
    pub verb: &'static str,
    pub answer: SeamAnswer,
}

impl SeamImpl {
    /// `led=fast`.
    pub fn atom(&self) -> String {
        format!("{}={}", self.label, self.implementation)
    }

    /// The primary declaration (see [`Self::decls`]).
    pub fn decl(&self) -> &'static lp_seam::SeamDecl {
        self.decls
            .first()
            .expect("an implementation answers at least one declared seam")
    }

    /// Whether this implementation answers declaration `id`.
    pub fn answers(&self, id: u16) -> bool {
        self.decls.iter().any(|d| d.id == id)
    }

    /// Whether `self` and `other` answer a declaration in common, so one
    /// request cannot hold both.
    pub fn overlaps(&self, other: &SeamImpl) -> bool {
        self.decls.iter().any(|d| other.answers(d.id))
    }

    /// The trace verb for a call of declaration `id`: [`Self::verb`] on a
    /// one-call implementation; on a multi-call one, the declaration's name
    /// without the label's prefix, `_` spelled `-` (`net_take_frame` →
    /// `take-frame`).
    pub fn verb_of(&self, id: u16) -> String {
        if self.decls.len() == 1 {
            return self.verb.to_string();
        }
        let name = self
            .decls
            .iter()
            .find(|d| d.id == id)
            .map_or("?", |d| d.name);
        let prefix = format!("{}_", self.label);
        name.strip_prefix(&prefix).unwrap_or(name).replace('_', "-")
    }

    /// Whether this is a test implementation (`lp_seam::test_seams`).
    pub fn is_test(&self) -> bool {
        lp_seam::test_seams::is_test_id(self.decl().id)
    }
}

/// `led=fast`: the LED performance seam (L1).
pub const LED_FAST: SeamImpl = SeamImpl {
    label: "led",
    implementation: "fast",
    decls: &[lp_seam::ws281x_wait_step::DECL],
    kind: SeamKind::Performance,
    verb: "wait-step",
    answer: SeamAnswer::ParkUntilInterrupt,
};

/// `net=lan`: the network seam, every call of it, answered from a virtual
/// LAN. A capability seam, so one of the defaults (softly, FD5).
pub const NET_LAN: SeamImpl = SeamImpl {
    label: "net",
    implementation: "lan",
    decls: lp_seam::net::CALLS,
    kind: SeamKind::Capability,
    verb: "net",
    answer: SeamAnswer::Net,
};

/// `test=echo` (feature `test-seams`).
#[cfg(feature = "test-seams")]
pub const TEST_ECHO: SeamImpl = SeamImpl {
    label: "test",
    implementation: "echo",
    decls: &[lp_seam::test_echo::DECL],
    kind: SeamKind::Performance,
    verb: "echo",
    answer: SeamAnswer::TestEcho,
};

/// `test=take` (feature `test-seams`).
#[cfg(feature = "test-seams")]
pub const TEST_TAKE: SeamImpl = SeamImpl {
    label: "test",
    implementation: "take",
    decls: &[lp_seam::test_take::DECL],
    kind: SeamKind::Capability,
    verb: "take",
    answer: SeamAnswer::Take,
};

/// Every implementation this emulator has, in label order.
pub fn implementations() -> &'static [SeamImpl] {
    #[cfg(not(feature = "test-seams"))]
    {
        &[LED_FAST, NET_LAN]
    }
    #[cfg(feature = "test-seams")]
    {
        &[LED_FAST, NET_LAN, TEST_ECHO, TEST_TAKE]
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
/// a performance one and never a test one. Today that is `net=lan`: a request
/// adds it softly, so every run scans its flash once per chip start, engages
/// the network seam when the image carries it, and otherwise says
/// `SEAM none engaged: …` and runs with no network.
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
    fn the_defaults_are_net_lan_and_never_a_performance_or_test_seam() {
        let atoms: Vec<String> = capability_defaults().map(SeamImpl::atom).collect();
        assert_eq!(atoms, ["net=lan"]);
        for i in capability_defaults() {
            assert_eq!(i.kind, SeamKind::Capability);
            assert!(!i.is_test());
        }
    }

    #[test]
    fn every_implementation_answers_declared_seams_of_its_kind() {
        for i in implementations() {
            for d in i.decls {
                assert_eq!(d.kind, i.kind, "{} answers {}", i.atom(), d.name);
                assert_eq!(lp_seam::SeamDecl::by_id(d.id), Some(d));
            }
            assert_ne!(i.implementation, "real");
        }
        for (n, a) in implementations().iter().enumerate() {
            for b in &implementations()[n + 1..] {
                assert!(!a.overlaps(b), "{} and {}", a.atom(), b.atom());
            }
        }
    }

    #[test]
    fn net_lan_answers_every_network_call_with_its_byte_on_net_mac() {
        let net = find("net", "lan").unwrap();
        assert_eq!(net.decls.len(), lp_seam::net::CALLS.len());
        assert_eq!(
            net.decl().id,
            lp_seam::net_mac::ID,
            "the primary carries the byte"
        );
        for d in lp_seam::net::CALLS {
            assert!(net.answers(d.id), "{}", d.name);
        }
        assert_eq!(net.verb_of(lp_seam::net_take_frame::ID), "take-frame");
        assert_eq!(net.verb_of(lp_seam::net_mac::ID), "mac");
        assert_eq!(LED_FAST.verb_of(lp_seam::ws281x_wait_step::ID), "wait-step");
        assert!(!net.is_test());
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
