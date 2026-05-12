//! Re-export async client from client module.

#[allow(
    unused_imports,
    reason = "Command modules import the client through this shim"
)]
pub use crate::client::LpClient;
