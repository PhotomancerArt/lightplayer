use core::future::Future;
use core::pin::Pin;

use crate::{UxAction, UxResult};

pub trait UxContext {
    fn dispatch(&mut self, action: UxAction) -> Pin<Box<dyn Future<Output = UxResult> + '_>>;
}
