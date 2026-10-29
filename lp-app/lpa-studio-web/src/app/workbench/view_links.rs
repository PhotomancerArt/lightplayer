//! The workbench's view addresses, in a context every component under the
//! frame can read.
//!
//! The view tabs are plain links ([`WorkbenchHrefs`]), and a few surfaces
//! inside the views link to another view the same way — the fixture face's
//! "Edit mapping" opens the Graph. The frame provides its hrefs here once
//! ([`use_provide_view_links`]) and such a surface asks for one view's
//! address ([`use_view_href`]) instead of threading the route through every
//! card. A link, not an action: it navigates and changes nothing.
//!
//! Outside the frame (a face story, a docs embed) there is no provider and
//! no address, so the link is absent — the same answer a route-less
//! workbench gives its hidden tabs.

use dioxus::prelude::*;

use super::{WorkbenchHrefs, WorkbenchView};

/// The frame's view hrefs, shared through Dioxus context.
#[derive(Clone, Copy)]
struct ViewLinks(Signal<WorkbenchHrefs>);

/// Provide `hrefs` to every component below the caller. Call it on every
/// render with the frame's current hrefs; readers are only notified when
/// they change (a lens address healing its slug or its device hint).
pub fn use_provide_view_links(hrefs: &WorkbenchHrefs) {
    let mut links = use_context_provider(|| ViewLinks(Signal::new(hrefs.clone()))).0;
    if *links.peek() != *hrefs {
        links.set(hrefs.clone());
    }
}

/// `view`'s address on the current lens, or `None` outside a workbench
/// frame or where the frame does not address it.
pub fn use_view_href(view: WorkbenchView) -> Option<String> {
    let links = use_hook(try_consume_context::<ViewLinks>)?;
    links.0.read().href(view)
}
