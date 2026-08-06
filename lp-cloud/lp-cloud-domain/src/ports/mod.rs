//! The effects the domain takes by injection.
//!
//! Four ports, and no others: [`meta_store::MetaStore`] (all service state,
//! one consistency domain), [`blob_store::BlobStore`] (content bytes, a
//! separate plane the domain never reads), [`clock::Clock`] (f64 epoch
//! seconds), and [`id_mint::IdMint`] (random bytes). Adding a fifth that
//! smells like IO — an HTTP client, a task spawner, a sleep — means the
//! logic wanting it belongs at the edge instead.

pub mod blob_store;
pub mod clock;
pub mod id_mint;
pub mod meta_store;
