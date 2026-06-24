use core::future::Future;
use core::pin::Pin;

use crate::{UiAction, UxResult};

pub trait UxContext {
    fn dispatch(&mut self, action: UiAction) -> Pin<Box<dyn Future<Output = UxResult> + '_>>;
}
