//! Logging in to a core over channel 3's `L` messages.

pub mod login_client;

pub use login_client::{Credential, LoginClient, LoginEvent};
