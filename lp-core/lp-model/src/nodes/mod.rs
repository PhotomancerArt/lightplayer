pub mod handle;
pub mod kind;
pub mod specifier;

// Node type modules
pub mod fixture;
pub mod output;
pub mod shader;
pub mod texture;

pub use handle::NodeHandle;
pub use kind::NodeKind;
pub use specifier::NodeSpecifier;

use core::any::Any;

/// Node config trait - all node configs implement this
pub trait NodeConfig: core::fmt::Debug {
    fn kind(&self) -> NodeKind;

    /// Get reference to Any for downcasting
    fn as_any(&self) -> &dyn Any;
}
