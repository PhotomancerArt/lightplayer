//! Exploratory Studio web UI surfaces.
//!
//! This family is for design spikes and mockups that should be visible in
//! storybook but are not yet production `base`, `core`, or `app`
//! components.

#[cfg(feature = "stories")]
pub(crate) mod node_ui_stories;
#[cfg(all(feature = "stories", target_arch = "wasm32"))]
pub(crate) mod preview_lab;
// The preview-lab config/stats models are pure Rust and stay ungated so their
// unit tests run in default (non-stories) test builds; their only non-test
// consumer is the stories-gated preview lab.
#[cfg_attr(
    not(all(feature = "stories", target_arch = "wasm32")),
    expect(
        dead_code,
        reason = "consumed by the stories-gated preview lab; kept ungated for host tests"
    )
)]
pub(crate) mod preview_lab_config;
#[cfg_attr(
    not(all(feature = "stories", target_arch = "wasm32")),
    expect(
        dead_code,
        reason = "consumed by the stories-gated preview lab; kept ungated for host tests"
    )
)]
pub(crate) mod preview_lab_stats;
