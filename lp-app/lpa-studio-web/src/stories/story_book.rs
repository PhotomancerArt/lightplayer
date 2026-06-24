use dioxus::prelude::*;

use crate::stories::story_registry::{DEFAULT_STORY_ID, all_stories, render_story, story_by_id};

#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StoryBook() -> Element {
    let initial_route = selected_story_route_from_hash();
    let mut selected_story_id = use_signal(move || initial_route.story_id);
    let mut viewport = use_signal(move || initial_route.viewport);
    let selected = selected_story_id.read().clone();
    let selected_viewport = *viewport.read();
    let descriptor = story_by_id(&selected).unwrap_or_else(|| {
        story_by_id(DEFAULT_STORY_ID).expect("default story descriptor is registered")
    });
    let stories = all_stories();
    let story_groups = story_groups(&stories);

    if is_story_png_mode() {
        return rsx! {
            main { class: "story-png-page",
                StoryCanvas {
                    story_id: descriptor.id,
                    label: descriptor.label,
                    description: descriptor.description,
                    frame_style: story_png_viewport().frame_style(),
                }
            }
        };
    }

    let frame_style = selected_viewport.frame_style();
    rsx! {
        main { class: "story-book",
            aside { class: "story-sidebar",
                div { class: "story-sidebar-heading",
                    h1 { "Studio Stories" }
                    p { "{stories.len()} component states" }
                }
                nav { class: "story-nav",
                    for group in story_groups {
                        section { class: "story-nav-group",
                            h2 { "{group.label}" }
                            div { class: "story-nav-links",
                                for story in group.stories {
                                    {
                                        let story_id = story.id;
                                        let link_class = if story.id == selected {
                                            "story-nav-link is-active"
                                        } else {
                                            "story-nav-link"
                                        };
                                        let story_href = story_hash(story_id, selected_viewport);
                                        rsx! {
                                            a {
                                                class: "{link_class}",
                                                href: "{story_href}",
                                                onclick: move |_| selected_story_id.set(story_id.to_string()),
                                                "{story.label}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            section { class: "story-stage",
                div { class: "story-toolbar",
                    div {
                        h2 { "{descriptor.label}" }
                        p { "{descriptor.family_label()} / {descriptor.id}" }
                    }
                    div { class: "story-viewport-controls",
                        for target_viewport in [StoryViewport::Sm, StoryViewport::Md, StoryViewport::Lg] {
                            {
                                let selected_for_button = selected.clone();
                                rsx! {
                                    ViewportButton {
                                        viewport: target_viewport,
                                        active: selected_viewport == target_viewport,
                                        onclick: move |_| {
                                            viewport.set(target_viewport);
                                            set_story_hash(&selected_for_button, target_viewport);
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
                StoryCanvas {
                    story_id: descriptor.id,
                    label: descriptor.label,
                    description: descriptor.description,
                    frame_style,
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct StoryRoute {
    story_id: String,
    viewport: StoryViewport,
}

#[derive(Clone, Debug)]
struct StoryGroup {
    label: &'static str,
    stories: Vec<crate::stories::story::StoryDescriptor>,
}

fn story_groups(stories: &[crate::stories::story::StoryDescriptor]) -> Vec<StoryGroup> {
    let mut groups = Vec::<StoryGroup>::new();
    for story in stories {
        let label = story.family_label();
        if let Some(group) = groups.iter_mut().find(|group| group.label == label) {
            group.stories.push(*story);
        } else {
            groups.push(StoryGroup {
                label,
                stories: vec![*story],
            });
        }
    }
    groups.sort_by(|left, right| {
        story_group_order(left.label)
            .cmp(&story_group_order(right.label))
            .then_with(|| left.label.cmp(right.label))
    });
    groups
}

fn story_group_order(label: &str) -> usize {
    match label {
        "Base" => 0,
        "Core" => 1,
        "Studio" => 2,
        "Exploration" => 3,
        _ => 99,
    }
}

pub fn should_show_story_book() -> bool {
    location_hash().is_some_and(|hash| hash.starts_with("#/stories"))
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StoryCanvas(
    story_id: &'static str,
    label: &'static str,
    description: &'static str,
    frame_style: &'static str,
) -> Element {
    rsx! {
        div {
            class: "story-canvas-shell",
            "data-story-capture": "1",
            "data-story-id": "{story_id}",
            "data-story-label": "{label}",
            div { class: "story-canvas-meta",
                h3 { "{label}" }
                p { "{description}" }
            }
            div { class: "story-frame", style: "{frame_style}",
                {render_story(story_id)}
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ViewportButton(
    viewport: StoryViewport,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if active {
        "story-viewport-button is-active"
    } else {
        "story-viewport-button"
    };
    rsx! {
        button {
            class,
            type: "button",
            onclick: move |event| onclick.call(event),
            span { class: "story-viewport-label", "{viewport.slug()}" }
            span { class: "story-viewport-detail", "{viewport.width_label()}" }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StoryViewport {
    Sm,
    Md,
    Lg,
}

impl StoryViewport {
    fn frame_style(self) -> &'static str {
        match self {
            Self::Sm => "max-width: 390px;",
            Self::Md => "max-width: 720px;",
            Self::Lg => "max-width: 1080px;",
        }
    }

    const fn slug(self) -> &'static str {
        match self {
            Self::Sm => "sm",
            Self::Md => "md",
            Self::Lg => "lg",
        }
    }

    const fn width_label(self) -> &'static str {
        match self {
            Self::Sm => "390 px",
            Self::Md => "720 px",
            Self::Lg => "1080 px",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "sm" => Some(Self::Sm),
            "md" => Some(Self::Md),
            "lg" => Some(Self::Lg),
            _ => None,
        }
    }
}

fn selected_story_route_from_hash() -> StoryRoute {
    location_hash()
        .and_then(|hash| parse_story_hash(&hash))
        .unwrap_or_else(|| StoryRoute {
            story_id: DEFAULT_STORY_ID.to_string(),
            viewport: StoryViewport::Lg,
        })
}

fn parse_story_hash(hash: &str) -> Option<StoryRoute> {
    let route = hash.strip_prefix("#/stories/")?;
    let (story_id, query) = route.split_once('?').unwrap_or((route, ""));
    let story_id = story_by_id(story_id).map(|story| story.id.to_string())?;
    let viewport = query
        .split('&')
        .filter_map(|part| part.split_once('='))
        .find_map(|(key, value)| {
            (key == "viewport")
                .then(|| StoryViewport::parse(value))
                .flatten()
        })
        .unwrap_or(StoryViewport::Lg);
    Some(StoryRoute { story_id, viewport })
}

fn story_hash(story_id: &str, viewport: StoryViewport) -> String {
    format!("#/stories/{story_id}?viewport={}", viewport.slug())
}

fn set_story_hash(story_id: &str, viewport: StoryViewport) {
    if let Some(location) = web_sys::window().map(|window| window.location()) {
        let _ = location.set_hash(&story_hash(story_id, viewport));
    }
}

fn story_png_viewport() -> StoryViewport {
    web_sys::window()
        .map(|window| window.location())
        .and_then(|location| location.search().ok())
        .and_then(|search| {
            search
                .trim_start_matches('?')
                .split('&')
                .filter_map(|part| part.split_once('='))
                .find_map(|(key, value)| {
                    (key == "viewport")
                        .then(|| StoryViewport::parse(value))
                        .flatten()
                })
        })
        .unwrap_or(StoryViewport::Lg)
}

fn is_story_png_mode() -> bool {
    web_sys::window()
        .map(|window| window.location())
        .and_then(|location| location.search().ok())
        .is_some_and(|search| {
            search
                .trim_start_matches('?')
                .split('&')
                .any(|part| part == "story-png=1")
        })
}

fn location_hash() -> Option<String> {
    web_sys::window()
        .map(|window| window.location())
        .and_then(|location| location.hash().ok())
}
