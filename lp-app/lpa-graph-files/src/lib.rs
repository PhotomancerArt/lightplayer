//! The mapping lab's files: real LightPlayer projects, opened into the
//! lab's model and saved back, in the product's own formats (vision D51;
//! the design is fit-check §6).
//!
//! - [`Opened::open`] reads `project.json`, the root module and every node
//!   it refers to into a lab [`Project`](lab_mapping_model::Project) —
//!   fixtures with their mappings (parts and patching), outputs with their ports,
//!   modules, and every other node as a plain card — and keeps what the lab
//!   doesn't model in a [`ProductProject`], untouched.
//! - [`ProductProject::save`] writes back what changed: each fixture's
//!   mapping with `lpc-mapping`'s own map2d format 6 writer, and the
//!   module, fixture and output files the Graph edits (mapping rebuild M4
//!   P05); an unedited project writes nothing.
//! - [`embedded`] — the test projects, baked in for the page.
//!
//! A fixture's mapping is `lpc-mapping`'s own (`map2d6::read`), the reader
//! the engine runs; a project at another project format, or a mapping in an
//! older map2d format, is refused by name.

pub mod channels;
pub mod embedded;
pub mod error;
pub mod json;
pub mod product_project;
pub mod save;
pub mod source;
pub mod stand_in_shader;

pub use error::FilesError;
pub use product_project::{MapFile, NodeFile, Opened, ProductProject};
pub use save::Saved;
pub use source::{DirFiles, MemFiles, Source};
