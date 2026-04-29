pub mod fixture;
pub mod kind;
pub mod output;
pub mod shader;
pub mod texture;

pub use kind::NodeKind;

use core::any::Any;

pub trait NodeConfig: core::fmt::Debug {
    fn kind(&self) -> NodeKind;
    fn as_any(&self) -> &dyn Any;
}
