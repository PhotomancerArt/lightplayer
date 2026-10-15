//! The mapping lab's files: real LightPlayer projects, opened into the
//! lab's model and saved back, in the product's own formats (vision D51;
//! the design is fit-check §6).
//!
//! - [`Opened::open`] reads `project.json`, the root module and every node
//!   it refers to into a lab [`Project`](lab_mapping_model::Project) —
//!   fixtures with their mappings and patches, outputs with their ports,
//!   modules, and every other node as a plain card — and keeps what the lab
//!   doesn't model in a [`ProductProject`], untouched.
//! - [`ProductProject::save`] writes the lab's project back: only the files
//!   it owns, only those that changed, each starting from the JSON it was
//!   read from.
//! - [`embedded`] — the three test projects, baked in for the page.
//!
//! **The fence:** this crate depends on the lab model and `serde_json`
//! only — no product crate. The product's document shapes are re-created
//! here ([`map2d_doc`], [`patch_doc`], [`channels`]); `lab-mapping-fit-tests`,
//! which may depend on the product, proves they agree (same lamps, same
//! wire, files untouched).

pub mod channels;
pub mod embedded;
pub mod error;
pub mod json;
pub mod map2d_doc;
pub mod patch_doc;
pub mod product_project;
pub mod save;
pub mod source;

pub use error::FilesError;
pub use product_project::{Opened, ProductProject};
pub use save::Saved;
pub use source::{DirFiles, MemFiles, Source};
