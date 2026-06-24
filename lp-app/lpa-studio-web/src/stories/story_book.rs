use dioxus::prelude::*;

use crate::stories::story::StoryDescriptor;
use crate::stories::story_registry::{all_stories, render_story, story_by_id, DEFAULT_STORY_ID};

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
                    h1 { "Lightplayer Stories" }
                    p { "{stories.len()} component states" }
                }
                div { class: "story-discovery-links", "aria-hidden": "true",
                    for story in stories.iter() {
                        {
                            let story_href = story_hash(story.id, selected_viewport);
                            rsx! {
                                a {
                                    href: "{story_href}",
                                    tabindex: "-1",
                                    "{story.label}"
                                }
                            }
                        }
                    }
                }
                nav { class: "story-nav",
                    for family in story_groups {
                        section { class: "story-nav-family",
                            h2 { "{family.label}" }
                            div { class: "story-nav-family-body",
                                for category in family.categories {
                                    {
                                        rsx! {
                                            div { class: "story-nav-category",
                                                if let Some(category_label) = category.label.as_deref() {
                                                    h3 { "{category_label}" }
                                                }
                                                div { class: "story-nav-components",
                                                    for component in category.components {
                                                        {
                                                            let first_story_id = component
                                                                .stories
                                                                .first()
                                                                .map(|story| story.id)
                                                                .unwrap_or(DEFAULT_STORY_ID);
                                                            let expanded = component
                                                                .stories
                                                                .iter()
                                                                .any(|story| story.id == selected);
                                                            let component_class = if expanded {
                                                                "story-nav-component is-active"
                                                            } else {
                                                                "story-nav-component"
                                                            };
                                                            let component_href = story_hash(first_story_id, selected_viewport);
                                                            rsx! {
                                                                div { class: "story-nav-component-group",
                                                                    a {
                                                                        class: "{component_class}",
                                                                        href: "{component_href}",
                                                                        onclick: move |_| selected_story_id.set(first_story_id.to_string()),
                                                                        span { class: "story-nav-component-label", "{component.label}" }
                                                                        span { class: "story-nav-component-count", "{component.stories.len()}" }
                                                                    }
                                                                    {
                                                                        let story_list_class = if expanded {
                                                                            "story-nav-story-list is-expanded"
                                                                        } else {
                                                                            "story-nav-story-list"
                                                                        };
                                                                        rsx! {
                                                                            div {
                                                                                class: "{story_list_class}",
                                                                                "aria-hidden": if expanded { "false" } else { "true" },
                                                                                div { class: "story-nav-story-list-inner",
                                                                                    for story in component.stories {
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
                                                                                                    tabindex: if expanded { "0" } else { "-1" },
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
                                                            }
                                                        }
                                                    }
                                                }
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
struct StoryFamilyGroup {
    key: &'static str,
    label: &'static str,
    categories: Vec<StoryCategoryGroup>,
}

#[derive(Clone, Debug)]
struct StoryCategoryGroup {
    key: Option<&'static str>,
    label: Option<String>,
    components: Vec<StoryComponentGroup>,
}

#[derive(Clone, Debug)]
struct StoryComponentGroup {
    key: &'static str,
    label: String,
    stories: Vec<StoryDescriptor>,
}

fn story_groups(stories: &[StoryDescriptor]) -> Vec<StoryFamilyGroup> {
    let mut groups = Vec::<StoryFamilyGroup>::new();
    for story in stories {
        let family_index = groups
            .iter()
            .position(|group| group.key == story.family)
            .unwrap_or_else(|| {
                groups.push(StoryFamilyGroup {
                    key: story.family,
                    label: story.family_label(),
                    categories: Vec::new(),
                });
                groups.len() - 1
            });
        let family = &mut groups[family_index];
        let category_index = family
            .categories
            .iter()
            .position(|category| category.key == story.category)
            .unwrap_or_else(|| {
                family.categories.push(StoryCategoryGroup {
                    key: story.category,
                    label: story.category.map(segment_label),
                    components: Vec::new(),
                });
                family.categories.len() - 1
            });
        let category = &mut family.categories[category_index];
        let component_index = category
            .components
            .iter()
            .position(|component| component.key == story.component)
            .unwrap_or_else(|| {
                category.components.push(StoryComponentGroup {
                    key: story.component,
                    label: segment_label(story.component),
                    stories: Vec::new(),
                });
                category.components.len() - 1
            });
        category.components[component_index].stories.push(*story);
    }
    groups.sort_by(|left, right| {
        story_group_order(left.key)
            .cmp(&story_group_order(right.key))
            .then_with(|| left.label.cmp(right.label))
    });
    groups
}

fn story_group_order(family: &str) -> usize {
    match family {
        "base" => 0,
        "core" => 1,
        "studio" => 2,
        "exploration" => 3,
        _ => 99,
    }
}

fn segment_label(segment: &str) -> String {
    segment
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| match part {
            "ui" => "UI".to_string(),
            "ux" => "UX".to_string(),
            "usb" => "USB".to_string(),
            "esp32" => "ESP32".to_string(),
            _ => {
                let mut chars = part.chars();
                let Some(first) = chars.next() else {
                    return String::new();
                };
                let mut label = first.to_ascii_uppercase().to_string();
                label.push_str(&chars.as_str().to_ascii_lowercase());
                label
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
                if !description.is_empty() {
                    p { "{description}" }
                }
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
