//! Generated docs checks and the compile-time docs link surface.
//!
//! `lpa-studio-web/build.rs` scans every article registered in
//! [`super::PAGES`] and emits this module's body: [`docs_links`], one
//! `pub const` per page and heading anchor, plus a test module asserting
//! that every `embed` fence names a registered directive
//! ([`super::embeds::EMBED_NAMES`]), every `sim=` argument names a sim its
//! page declares, and every `#/docs/...` link resolves to a real page and
//! anchor.
//!
//! There is nothing hand-written here on purpose — the source of truth is
//! `docs/user-guide/` plus the manifest. Failures name the article file and
//! the offending reference; fix the article (or the manifest), never the
//! generated file.

include!(concat!(env!("OUT_DIR"), "/docs_checks.generated.rs"));
