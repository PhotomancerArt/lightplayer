//! The web reports **place** to core: the page the router shows and the
//! drawers and panels open over it (roadmap M7, D3: "the core knows but its
//! read only").
//!
//! Navigation stays here. The router owns the route and the chrome owns
//! its open flags; one effect ([`use_report_place`]) folds them into a
//! [`UiPlace`] and sends `StudioCommand::Place` whenever the result
//! changes — latest wins, like `BluetoothReach`. Core only reads it (the
//! agent's readout leads with it, ⌘K ranks by it); it never routes or
//! opens anything because of it.
//!
//! What core already owns is not reported: the focused node, each node
//! card's open sections and the patch surface's selection.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use lpa_studio_core::app::studio::studio_view_channel::CommandSender;
use lpa_studio_core::{StudioCommand, UiPage, UiPanel, UiPlace, UiProjectView, UiSessionSection};

use crate::router::{ProjectView, StudioRoute};

/// Which section the header session control's panel shows while it is
/// open (`None` while it is closed). The control writes it; the reporter
/// reads it. Provided once by the web app; stories go without.
#[derive(Clone, Copy)]
pub(crate) struct SessionPanelPlace(pub Signal<Option<UiSessionSection>>);

/// Provide the session panel's place slot to everything below the caller.
pub(crate) fn use_provide_session_panel_place() -> SessionPanelPlace {
    use_context_provider(|| SessionPanelPlace(Signal::new(None)))
}

/// The session panel's place slot, when a web app provides one.
pub(crate) fn use_session_panel_place() -> Option<SessionPanelPlace> {
    use_hook(try_consume_context::<SessionPanelPlace>)
}

/// Report place to core whenever the route or an open flag changes.
pub(crate) fn use_report_place(
    tx: CommandSender,
    route: Signal<StudioRoute>,
    app_chat_open: Signal<bool>,
    palette_open: Signal<bool>,
    session_panel: SessionPanelPlace,
) {
    let sent: Rc<RefCell<Option<UiPlace>>> = use_hook(|| Rc::new(RefCell::new(None)));
    use_effect(move || {
        let place = place_of(
            &route(),
            app_chat_open(),
            palette_open(),
            (session_panel.0)(),
        );
        let mut sent = sent.borrow_mut();
        if sent.as_ref() != Some(&place) {
            *sent = Some(place.clone());
            tx.send(StudioCommand::Place(place));
        }
    });
}

/// The place the page is at: `route`'s page, with what is open over it.
fn place_of(
    route: &StudioRoute,
    app_chat_open: bool,
    palette_open: bool,
    session_panel: Option<UiSessionSection>,
) -> UiPlace {
    let mut place = UiPlace::new(page_of(route));
    if app_chat_open {
        place = place.with_panel(UiPanel::AppChat);
    }
    if palette_open {
        place = place.with_panel(UiPanel::CommandPalette);
    }
    if let Some(section) = session_panel {
        place = place.with_panel(UiPanel::Session(section));
    }
    place
}

/// The page a route shows, in core's words. Exhaustive on purpose: a new
/// route does not compile until it says which page it is.
pub(crate) fn page_of(route: &StudioRoute) -> UiPage {
    match route {
        StudioRoute::Home => UiPage::Home,
        StudioRoute::Devices => UiPage::Devices,
        StudioRoute::Projects => UiPage::Projects,
        StudioRoute::Explore => UiPage::Explore,
        StudioRoute::Account => UiPage::Account,
        StudioRoute::Unlock => UiPage::Unlock,
        StudioRoute::Project { uid, view, .. } => UiPage::Project {
            uid: uid.to_string(),
            view: project_view(*view),
        },
        StudioRoute::Example { slug, view, .. } => UiPage::Example {
            slug: slug.clone(),
            view: project_view(*view),
        },
        StudioRoute::Device { uid, view } => UiPage::Device {
            uid: uid.clone(),
            view: project_view(*view),
        },
        StudioRoute::Stories { .. } => UiPage::Stories,
        StudioRoute::Boards { board } => UiPage::Boards {
            board: board.clone(),
        },
        StudioRoute::BoardEditor => UiPage::BoardEditor,
        StudioRoute::Docs { page, .. } => UiPage::Docs {
            article: page.clone(),
        },
    }
}

fn project_view(view: ProjectView) -> UiProjectView {
    match view {
        ProjectView::Workspace => UiProjectView::Nodes,
        ProjectView::Play => UiProjectView::Play,
        ProjectView::Patch => UiProjectView::Patch,
        ProjectView::Mapping => UiProjectView::Mapping,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_reads_as_its_page_with_its_view() {
        assert_eq!(page_of(&StudioRoute::parse("/devices")), UiPage::Devices);
        assert_eq!(
            page_of(&StudioRoute::parse("/device/dev123/patch")),
            UiPage::Device {
                uid: "dev123".to_string(),
                view: UiProjectView::Patch,
            }
        );
        assert_eq!(
            page_of(&StudioRoute::parse("/docs/wiring")),
            UiPage::Docs {
                article: Some("wiring".to_string())
            }
        );
    }

    #[test]
    fn open_chrome_rides_the_place() {
        let place = place_of(
            &StudioRoute::Home,
            true,
            false,
            Some(UiSessionSection::Changes),
        );
        assert_eq!(
            place,
            UiPlace::new(UiPage::Home)
                .with_panel(UiPanel::AppChat)
                .with_panel(UiPanel::Session(UiSessionSection::Changes))
        );
        assert_eq!(
            place_of(&StudioRoute::Home, false, false, None),
            UiPlace::new(UiPage::Home)
        );
    }
}
