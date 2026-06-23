/// Build/runtime availability for a provider descriptor.
///
/// The registry only constructs providers compiled for the current feature and
/// target matrix. This enum leaves room for future descriptors that are known
/// to the crate but not usable in the current runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkProviderAvailability {
    /// Provider can be constructed and used in the current build/runtime.
    Available,
    /// Provider is known but cannot currently be used.
    Unavailable { reason: &'static str },
}
