mod provider;

pub use provider::{FakeProvider, descriptor};

#[cfg(test)]
mod tests;
