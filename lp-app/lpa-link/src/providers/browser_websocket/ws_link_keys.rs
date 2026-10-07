//! Where this page's LAN links get their keys: one [`LinkKeys`] per page,
//! handed over by the app (Studio's access layer) and read by every
//! connection's key walk.
//!
//! Page-wide, like the wire flags (`wire_reader::packed_replies_wanted`):
//! every LAN link on a page presents the keys of the same browser. Until the
//! app installs its keys, links come up anonymous ([`NoLinkKeys`]) — an open
//! board is reachable, a locked one is not.

use std::cell::RefCell;
use std::rc::Rc;

use crate::providers::network_link::{LinkKeys, NoLinkKeys};

thread_local! {
    static KEYS: RefCell<Rc<dyn LinkKeys>> = RefCell::new(Rc::new(NoLinkKeys));
}

/// Install the page's key source. Links built after this present its keys;
/// links already up see its next generation on their next service pass.
pub fn set_link_keys(keys: Rc<dyn LinkKeys>) {
    KEYS.with(|slot| *slot.borrow_mut() = keys);
}

/// The page's key source.
pub(crate) fn link_keys() -> Rc<dyn LinkKeys> {
    KEYS.with(|slot| Rc::clone(&slot.borrow()))
}
