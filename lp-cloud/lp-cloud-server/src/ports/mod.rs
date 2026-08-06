//! The production adapters the edge injects into the domain.
//!
//! Two of them are real effects the domain refuses to perform for itself —
//! [`system_clock::SystemClock`] (wall time) and
//! [`secure_mint::SecureMint`] (OS randomness). The other two,
//! [`any_meta_store::AnyMetaStore`] and [`any_blob_store::AnyBlobStore`],
//! are not adapters at all but *selectors*: newtypes over a boxed trait
//! object, so one concrete `AppState` type can hold whichever backend the
//! configuration named.

pub mod any_blob_store;
pub mod any_meta_store;
pub mod secure_mint;
pub mod system_clock;

pub use any_blob_store::AnyBlobStore;
pub use any_meta_store::AnyMetaStore;
pub use secure_mint::SecureMint;
pub use system_clock::SystemClock;
