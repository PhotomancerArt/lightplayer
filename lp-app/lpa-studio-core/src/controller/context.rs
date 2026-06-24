use core::future::Future;
use core::pin::Pin;

use crate::{UiAction, UiResult};

pub trait ControllerContext {
    fn dispatch(&mut self, action: UiAction) -> Pin<Box<dyn Future<Output = UiResult> + '_>>;
}
