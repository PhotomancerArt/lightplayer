//! Canonical test doubles for the agent seams ([`FakeHost`],
//! [`ScriptedProvider`]).
//!
//! Compiled only for this crate's own unit tests (`cfg(test)`) and for
//! consumers of the `test-doubles` feature — `lpa-agent-harness` re-exports
//! this module as the shared harness surface. The doubles live HERE rather
//! than in the harness crate because in-src unit tests compile the crate a
//! second time under `cfg(test)`: a double defined in a downstream crate
//! implements the traits of the NORMAL lib build and can never satisfy the
//! test build's trait bounds (rustc: "multiple different versions of crate
//! `lpa_agent`"). One source file, instantiated per build, keeps every
//! suite on the same double.

pub mod fake_host;
pub mod scripted_provider;

pub use fake_host::FakeHost;
pub use scripted_provider::ScriptedProvider;
