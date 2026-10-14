//! Home's own verbs as offers: `project/new` and `project/open`.
//!
//! With no project open, Studio shows Home, and the two things a person
//! does there before anything else are start a project and open one. The
//! New menu's rows and the gallery's cards were the only ways to do either,
//! so the app agent, which sees the app through the offer tree, had no way
//! to start a project at all: `edit_project` refused with nothing open, and
//! nothing on offer would open one (the activity corpus, S4 and S18).
//!
//! Both follow the board-ids ADR's rule for a verb that takes a value: one
//! offer with a choice list, never one offer per choice
//! (`docs/adr/2026-10-02-board-ids-and-typed-offer-parameters.md`).
//!
//! - `project/new` takes `template` (the New menu's rows, blank by default)
//!   and an optional `name`, and binds the menu's own
//!   [`HomeOp::CreateProject`]: the package lands in the library and opens
//!   in the editor.
//! - `project/open` takes `project`, one of the library's projects by uid,
//!   and binds [`HomeOp::OpenPackage`] with no device hint — what a
//!   gallery card's link resolves to.
//!
//! Both are Routine: nothing is lost by making or opening a project (a
//! project open with unsaved edits is never on Home, so there is nothing
//! for an open to throw away).
//!
//! The paths sit in the `project` namespace on purpose. Home is shown only
//! while no project is loaded, so these are the only project verbs in the
//! tree whenever they are published, and the editor's own `project/…`
//! verbs never share the tree with them.

use crate::app::home::{HOME_NODE_ID, HomeOp, ProjectTemplate, UiHomeView, UiPackageCard};
use crate::{
    ControllerId, OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferParam, OfferPath,
    UiAction, UiOffer,
};

/// `project/new`'s template parameter.
pub const NEW_PROJECT_TEMPLATE_PARAM: &str = "template";
/// `project/new`'s optional name parameter.
pub const NEW_PROJECT_NAME_PARAM: &str = "name";
/// `project/open`'s project parameter.
pub const OPEN_PROJECT_PARAM: &str = "project";

/// What an empty name field means.
const NAME_PLACEHOLDER: &str = "Name (optional) \u{2014} else named after the template";

/// Why neither verb can be pressed without a library.
const NO_LIBRARY: &str = "your project library is not available in this browser";

/// Every verb Home offers, in the order it offers them.
pub fn home_offers(home: &UiHomeView) -> Vec<UiOffer> {
    vec![
        new_project_offer(home.library_available),
        open_project_offer(&home.projects, home.library_available),
    ]
}

/// `project/new`: create a project from a template and open it.
pub fn new_project_offer(library_available: bool) -> UiOffer {
    let path = OfferPath::project().child("new");
    let unbound = create_action(ProjectTemplate::Blank, None);
    if !library_available {
        return UiOffer::new(path, "add", unbound.disabled(NO_LIBRARY));
    }
    let options = ProjectTemplate::ALL
        .into_iter()
        .map(|template| {
            OfferChoice::new(template.tag(), template.label()).with_detail(template.description())
        })
        .collect();
    let params = vec![
        OfferParam::choice(
            NEW_PROJECT_TEMPLATE_PARAM,
            "template",
            options,
            Some(ProjectTemplate::Blank.tag().to_string()),
        ),
        OfferParam::text(NEW_PROJECT_NAME_PARAM, "name", NAME_PLACEHOLDER).optional(),
    ];
    let binder = OfferBinder::new(|args: &OfferArgs| {
        let tag = args.choice(NEW_PROJECT_TEMPLATE_PARAM).unwrap_or_default();
        let template =
            ProjectTemplate::from_tag(tag).ok_or_else(|| OfferArgError::NotAnOption {
                name: NEW_PROJECT_TEMPLATE_PARAM.to_string(),
                value: tag.to_string(),
                options: ProjectTemplate::ALL
                    .iter()
                    .map(|template| template.tag().to_string())
                    .collect(),
            })?;
        let name = args.text(NEW_PROJECT_NAME_PARAM).map(str::to_string);
        Ok(create_action(template, name))
    });
    UiOffer::with_params(path, "add", params, binder, unbound)
}

/// `project/open`: open one of the library's projects in the editor.
///
/// A project this Studio cannot open is listed disabled with what was
/// found, never hidden. With no project at all, the offer is published
/// disabled with the reason and no parameters (the push offer's rule).
pub fn open_project_offer(projects: &[UiPackageCard], library_available: bool) -> UiOffer {
    let path = OfferPath::project().child("open");
    let unbound = open_action(String::new());
    if !library_available {
        return UiOffer::new(path, "play", unbound.disabled(NO_LIBRARY));
    }
    if projects.is_empty() {
        return UiOffer::new(
            path,
            "play",
            unbound.disabled("your library has no projects yet"),
        );
    }
    let options = projects.iter().map(project_option).collect();
    let params = vec![OfferParam::choice(
        OPEN_PROJECT_PARAM,
        "project",
        options,
        None,
    )];
    let binder = OfferBinder::new(|args: &OfferArgs| {
        let uid = args.choice(OPEN_PROJECT_PARAM).unwrap_or_default();
        Ok(open_action(uid.to_string()))
    });
    UiOffer::with_params(path, "play", params, binder, unbound)
}

/// One library project as a choice: its uid, its title, and a line saying
/// what kind of project it is and which board it is for.
fn project_option(card: &UiPackageCard) -> OfferChoice {
    let mut detail = card.project_kind.clone();
    if let Some(target) = card
        .target
        .as_deref()
        .filter(|target| *target != crate::DESKTOP_BOARD_ID)
    {
        detail.push_str(&format!(
            " \u{b7} for {}",
            crate::app::roster::board_display_name(target)
        ));
    }
    if card.open_elsewhere {
        detail.push_str(" \u{b7} open in another tab");
    }
    let option = OfferChoice::new(&card.uid, &card.slug).with_detail(detail);
    match card.health.blocked() {
        Some((headline, _)) => option.disabled(headline),
        None => option,
    }
}

/// The New menu's create-and-open, read as the verb it is.
fn create_action(template: ProjectTemplate, name: Option<String>) -> UiAction {
    UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::CreateProject { template, name },
    )
    .with_label("New project")
}

/// A card's open, with no device hint: Studio picks where it runs.
fn open_action(key: String) -> UiAction {
    UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenPackage { key, prefer: None },
    )
    .with_label("Open project")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::library::PackageHealth;

    #[test]
    fn new_takes_a_template_and_a_name_and_binds_the_menus_create() {
        let offer = new_project_offer(true);
        assert_eq!(offer.path.to_string(), "project/new");
        assert!(offer.is_enabled(), "blank is preselected");
        assert!(offer.consequence().is_routine());
        let names: Vec<&str> = offer.params().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["template", "name"]);

        let blank = offer.press(&OfferArgs::new()).expect("defaults bind");
        assert_eq!(
            blank.op_as::<HomeOp>(),
            Some(&HomeOp::CreateProject {
                template: ProjectTemplate::Blank,
                name: None,
            })
        );
        let named = offer
            .press(
                &OfferArgs::new()
                    .with("template", "pattern-1d")
                    .with("name", "  Porch  "),
            )
            .expect("a template and a name bind");
        assert_eq!(
            named.op_as::<HomeOp>(),
            Some(&HomeOp::CreateProject {
                template: ProjectTemplate::Pattern1d,
                name: Some("Porch".to_string()),
            })
        );
        assert!(
            offer
                .press(&OfferArgs::new().with("template", "shader"))
                .is_err(),
            "a template that is not one is refused"
        );
    }

    #[test]
    fn every_template_is_a_choice_with_the_menus_words() {
        let offer = new_project_offer(true);
        let crate::OfferParamKind::Choice { options, .. } = &offer.params()[0].kind else {
            panic!("template is a choice");
        };
        for (option, template) in options.iter().zip(ProjectTemplate::ALL) {
            assert_eq!(option.value, template.tag());
            assert_eq!(option.label, template.label());
            assert_eq!(option.detail.as_deref(), Some(template.description()));
            assert_eq!(ProjectTemplate::from_tag(template.tag()), Some(template));
        }
        assert_eq!(options.len(), ProjectTemplate::ALL.len());
    }

    #[test]
    fn open_lists_the_library_and_binds_the_cards_open() {
        let blocked = PackageHealth::Blocked {
            headline: "Format 2 — too old for this Studio".to_string(),
            remedy: "Export it.".to_string(),
        };
        let projects = [
            card(
                "prj1",
                "porch",
                Some("seeed/xiao-esp32-c6"),
                PackageHealth::Ready,
            ),
            card("prj2", "old", None, blocked),
        ];
        let offer = open_project_offer(&projects, true);
        assert_eq!(offer.path.to_string(), "project/open");
        assert!(!offer.is_enabled(), "nothing preselected: choose a project");
        let crate::OfferParamKind::Choice { options, preselect } = &offer.params()[0].kind else {
            panic!("project is a choice");
        };
        assert_eq!(preselect, &None);
        assert_eq!(options[0].value, "prj1");
        assert_eq!(options[0].label, "porch");
        assert_eq!(
            options[0].detail.as_deref(),
            Some("General \u{b7} for XIAO ESP32-C6")
        );
        assert_eq!(
            options[1].disabled.as_deref(),
            Some("Format 2 — too old for this Studio"),
            "a project this Studio cannot open is listed, disabled"
        );

        let open = offer
            .press(&OfferArgs::new().with("project", "prj1"))
            .expect("a library project binds");
        assert_eq!(
            open.op_as::<HomeOp>(),
            Some(&HomeOp::OpenPackage {
                key: "prj1".to_string(),
                prefer: None,
            })
        );
        assert!(
            offer
                .press(&OfferArgs::new().with("project", "prj2"))
                .is_err()
        );
    }

    #[test]
    fn without_projects_or_a_library_the_verbs_say_why() {
        let empty = open_project_offer(&[], true);
        assert!(empty.params().is_empty());
        assert!(!empty.is_enabled());
        for offer in [new_project_offer(false), open_project_offer(&[], false)] {
            assert!(!offer.is_enabled(), "{}", offer.path);
            assert!(offer.params().is_empty());
        }
    }

    fn card(uid: &str, slug: &str, target: Option<&str>, health: PackageHealth) -> UiPackageCard {
        UiPackageCard {
            uid: uid.to_string(),
            kind: "Module".to_string(),
            project_kind: "General".to_string(),
            exports: Vec::new(),
            slug: slug.to_string(),
            last_saved_at: None,
            provenance: None,
            on_boards: Vec::new(),
            open_elsewhere: false,
            target: target.map(str::to_string),
            health,
        }
    }
}
