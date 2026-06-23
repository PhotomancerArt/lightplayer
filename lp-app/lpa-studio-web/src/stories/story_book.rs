use dioxus::prelude::*;

use crate::stories::story_registry::{DEFAULT_STORY_ID, all_stories, render_story, story_by_id};

#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StoryBook() -> Element {
    let initial_story_id = selected_story_id_from_hash();
    let mut selected_story_id = use_signal(move || initial_story_id);
    let mut viewport = use_signal(|| StoryViewport::Wide);
    let selected = selected_story_id.read().clone();
    let descriptor = story_by_id(&selected).unwrap_or_else(|| {
        story_by_id(DEFAULT_STORY_ID).expect("default story descriptor is registered")
    });
    let stories = all_stories();

    if is_story_png_mode() {
        return rsx! {
            main { class: "story-png-page",
                StoryCanvas {
                    story_id: descriptor.id,
                    label: descriptor.label,
                    description: descriptor.description,
                    frame_style: StoryViewport::Wide.frame_style(),
                }
            }
        };
    }

    let frame_style = viewport.read().frame_style();
    rsx! {
        main { class: "story-book",
            aside { class: "story-sidebar",
                div { class: "story-sidebar-heading",
                    h1 { "Studio Stories" }
                    p { "{stories.len()} component states" }
                }
                nav { class: "story-nav",
                    for story in stories.iter() {
                        {
                            let story_id = story.id;
                            let link_class = if story.id == selected {
                                "story-nav-link is-active"
                            } else {
                                "story-nav-link"
                            };
                            rsx! {
                                a {
                                    class: "{link_class}",
                                    href: "#/stories/{story.id}",
                                    onclick: move |_| selected_story_id.set(story_id.to_string()),
                                    span { class: "story-nav-group", "{story.group}" }
                                    strong { "{story.label}" }
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
                        p { "{descriptor.group} / {descriptor.id}" }
                    }
                    div { class: "story-viewport-controls",
                        ViewportButton {
                            label: "Narrow",
                            active: *viewport.read() == StoryViewport::Narrow,
                            onclick: move |_| viewport.set(StoryViewport::Narrow),
                        }
                        ViewportButton {
                            label: "Panel",
                            active: *viewport.read() == StoryViewport::Panel,
                            onclick: move |_| viewport.set(StoryViewport::Panel),
                        }
                        ViewportButton {
                            label: "Wide",
                            active: *viewport.read() == StoryViewport::Wide,
                            onclick: move |_| viewport.set(StoryViewport::Wide),
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
fn ViewportButton(label: &'static str, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
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
            "{label}"
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StoryViewport {
    Narrow,
    Panel,
    Wide,
}

impl StoryViewport {
    fn frame_style(self) -> &'static str {
        match self {
            Self::Narrow => "max-width: 390px;",
            Self::Panel => "max-width: 720px;",
            Self::Wide => "max-width: 1040px;",
        }
    }
}

fn selected_story_id_from_hash() -> String {
    location_hash()
        .and_then(|hash| hash.strip_prefix("#/stories/").map(str::to_string))
        .filter(|id| story_by_id(id).is_some())
        .unwrap_or_else(|| DEFAULT_STORY_ID.to_string())
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
