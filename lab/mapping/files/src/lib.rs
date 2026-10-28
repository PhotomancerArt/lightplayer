//! The mapping lab's files: real LightPlayer projects, opened into the
//! lab's model and saved back, in the product's own formats (vision D51;
//! the design is fit-check §6).
//!
//! - [`Opened::open`] reads `project.json`, the root module and every node
//!   it refers to into a lab [`Project`](lab_mapping_model::Project) —
//!   fixtures with their mappings (parts and patching), outputs with their ports,
//!   modules, and every other node as a plain card — and keeps what the lab
//!   doesn't model in a [`ProductProject`], untouched.
//! - Saving comes back with map2d format 6 written (mapping rebuild M4
//!   P05); until then the Graph reads and edits but saves nothing.
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
pub mod source;
pub mod stand_in_shader;

pub use error::FilesError;
pub use product_project::{Opened, ProductProject};
pub use source::{DirFiles, MemFiles, Source};
