use std::cell::RefCell;
use std::rc::Rc;

use crate::UxUpdate;

#[derive(Clone)]
pub struct UxUpdateSink {
    on_update: Rc<RefCell<dyn FnMut(UxUpdate)>>,
}

impl UxUpdateSink {
    pub fn new(on_update: impl FnMut(UxUpdate) + 'static) -> Self {
        Self {
            on_update: Rc::new(RefCell::new(on_update)),
        }
    }

    pub fn noop() -> Self {
        Self::new(|_| {})
    }

    pub fn emit(&self, update: UxUpdate) {
        (self.on_update.borrow_mut())(update);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::{StudioView, UxUpdate};

    use super::*;

    #[test]
    fn sink_accepts_mutating_callbacks() {
        let count = Rc::new(RefCell::new(0_u32));
        let sink = UxUpdateSink::new({
            let count = Rc::clone(&count);
            move |_| {
                *count.borrow_mut() += 1;
            }
        });

        sink.emit(UxUpdate::View(StudioView::new(Vec::new(), Vec::new())));
        sink.emit(UxUpdate::View(StudioView::new(Vec::new(), Vec::new())));

        assert_eq!(*count.borrow(), 2);
    }
}
