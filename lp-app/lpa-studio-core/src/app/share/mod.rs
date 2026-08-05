//! Share envelopes: projects and nodes as pasteable JSON.
//!
//! There is no cloud provider, so the clipboard and the filesystem are the
//! whole distribution story. Zip already moves a project between machines
//! ([`crate::app::library::export_package`]); these envelopes add the
//! paste-it-into-a-message channel, and — for a single node, including a
//! shader and its `.glsl` — the only channel there is.
//!
//! Both kinds lead with `{ "kind", "format" }` so a paste target can
//! classify a blob before committing to a shape ([`peek_header`]).
//!
//! **Versioned, not migrated.** `format` mismatches are rejected outright.
//! During alpha the tree moves too fast to carry migrations, and a loud
//! refusal beats silently misreading a neighbouring version's bytes. See
//! `docs/adr/2026-07-28-share-envelopes.md` for the decision and
//! `docs/debt/library-format-migration-gap.md` for the standing burden.
//!
//! That `format` is the ENVELOPE's version, and it is separate from the
//! PROJECT format of whatever the envelope carries:
//!
//! - [`PackageEnvelope`] carries a whole project including its
//!   `project.json`, so the format is already inside it. Import classifies
//!   and — where it can — migrates it before installing
//!   ([`crate::app::library::import_json`]).
//! - [`NodeEnvelope`] carries one node def and its assets, with no
//!   manifest, so it states its own `artifact_format`
//!   ([`lpc_model::PROJECT_FORMAT_VERSION`] at copy time). Nothing can
//!   migrate a bare node, so a mismatched or unstamped one is refused on
//!   decode with a message that names the remedy.
//!
//! Sans-IO: these are pure functions over bytes. The clipboard lives in the
//! web edge (`lpa-studio-web/src/clipboard.rs`), and installing a decoded
//! package is the library's existing `install_files_with_fresh_uid` path.

pub mod node_envelope;
pub mod package_envelope;
pub mod share_envelope;
pub mod share_error;
pub mod share_file;

pub use node_envelope::NodeEnvelope;
pub use package_envelope::PackageEnvelope;
pub use share_envelope::{NODE_KIND, PACKAGE_KIND, SHARE_FORMAT_VERSION, ShareHeader, peek_header};
pub use share_error::ShareError;
pub use share_file::ShareFile;
