use dioxus::prelude::*;

use crate::stories::story::StoryDescriptor;
use crate::stories::story_registry::{
    DEFAULT_STORY_ID, all_stories, generated_at_utc, render_story, story_by_id,
};

#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StoryBook() -> Element {
    // Stories render thumbs statically: no PreviewHost boot, no live
    // badges racing capture (see `StaticThumbPreviews`).
    use_context_provider(|| crate::app::home::gallery_preview::StaticThumbPreviews);
    let initial_route = selected_story_route_from_url();
    let mut selected_story_id = use_signal(move || initial_route.story_id);
    let mut viewport = use_signal(move || initial_route.viewport);
    let _route_listener =
        use_hook(move || install_story_route_listener(selected_story_id, viewport));
    let stories = all_stories();
    let story_groups = story_groups(&stories);
    let selected = selected_story_id.read().clone();
    let selected_viewport = *viewport.read();
    // Never blank the page: an unknown route falls back to the default,
    // and a STALE default (one whose story was deleted) falls back to the
    // first registered story. The old `.expect` turned a stale default
    // into an empty body — which reads to the capture pipeline as "no
    // stories exist" rather than "the book is broken".
    let selection = story_selection(&selected, &story_groups)
        .or_else(|| story_selection(DEFAULT_STORY_ID, &story_groups))
        .or_else(|| first_story_selection(&story_groups))
        .expect("at least one story is registered");
    let build_stamp = generated_at_utc();
    let story_summary = format!("{} states / sm md lg", stories.len());
    let page_title = selection.label();
    let page_description = selection.description();
    let page_source_ref = selection.source_ref();
    let page_id = selection.id().to_string();

    if is_story_png_mode() {
        let story_viewport = story_png_viewport();
        return rsx! {
            main { class: "tw:p-[22px]",
                {render_story_selection(&selection, story_viewport)}
            }
        };
    }

    rsx! {
        main { class: "tw:grid tw:h-screen tw:min-h-0 tw:grid-cols-[260px_minmax(0,1fr)] tw:overflow-hidden tw:max-[880px]:grid-cols-1",
            aside { class: "tw:min-h-0 tw:overflow-y-auto tw:border-r tw:border-border tw:bg-card-subtle tw:max-[880px]:border-b tw:max-[880px]:border-r-0",
                header { class: "tw:grid tw:gap-2 tw:border-b tw:border-border-muted tw:bg-[linear-gradient(135deg,var(--studio-color-surface-muted),var(--studio-status-good-bg)_54%,var(--studio-color-surface-subtle))] tw:px-[18px] tw:py-4",
                    h1 { class: "tw:m-0 tw:text-lg tw:font-extrabold tw:leading-tight tw:text-strong-foreground", "Lightplayer Design" }
                    div { class: "tw:grid tw:gap-1",
                        p { class: "tw:m-0 tw:font-mono tw:text-[0.68rem] tw:leading-tight tw:text-muted-foreground", "built {build_stamp}" }
                        p { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:leading-tight tw:text-subtle-foreground", "{story_summary}" }
                    }
                }
                div { class: "tw:hidden", "aria-hidden": "true",
                    for family in story_groups.iter() {
                        for category in family.categories.iter() {
                            for component in category.components.iter() {
                                {
                                    let overview_href = story_href(&component.overview_id, selected_viewport);
                                    rsx! {
                                        a {
                                            href: "{overview_href}",
                                            tabindex: "-1",
                                            "{component.label} overview"
                                        }
                                    }
                                }
                                for story in component.stories.iter() {
                                    {
                                        let story_link = story_href(story.id, selected_viewport);
                                        // The capture script scrapes these links to
                                        // discover stories; the flag rides along so it
                                        // can capture screenshot stories at lg only.
                                        rsx! {
                                            a {
                                                href: "{story_link}",
                                                tabindex: "-1",
                                                "data-story-screenshot": if story.screenshot { "1" } else { "0" },
                                                "{story.label}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                nav { class: "tw:grid tw:gap-[18px] tw:p-[18px]",
                    for family in story_groups.iter() {
                        section { class: "tw:grid tw:min-w-0 tw:gap-2",
                            h2 { class: "tw:m-0 tw:mb-0.5 tw:text-xs tw:font-extrabold tw:uppercase tw:text-heading", "{family.label}" }
                            div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                                for category in family.categories.iter() {
                                    {
                                        rsx! {
                                            div { class: "tw:grid tw:min-w-0 tw:gap-1 tw:border-l tw:border-border-muted tw:pl-2.5",
                                                if let Some(category_label) = category.label.as_deref() {
                                                    h3 { class: "tw:m-0 tw:mt-2 tw:text-xs tw:font-extrabold tw:uppercase tw:text-subtle-foreground", "{category_label}" }
                                                }
                                                div { class: "tw:grid tw:min-w-0 tw:gap-1",
                                                    for component in category.components.iter() {
                                                        {
                                                            let expanded = component.overview_id == selected || component
                                                                .stories
                                                                .iter()
                                                                .any(|story| story.id == selected);
                                                            let component_class = story_nav_component_class(expanded);
                                                            let component_href = story_href(&component.overview_id, selected_viewport);
                                                            let overview_id_for_component = component.overview_id.clone();
                                                            rsx! {
                                                                div { class: "tw:grid tw:min-w-0",
                                                                    a {
                                                                        class: "{component_class}",
                                                                        href: "{component_href}",
                                                                        onclick: move |_| selected_story_id.set(overview_id_for_component.clone()),
                                                                        span { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap", "{component.label}" }
                                                                        span { class: "tw:text-xs tw:text-subtle-foreground", "{component.stories.len()}" }
                                                                    }
                                                                    {
                                                                        let story_list_class = if expanded {
                                                                            story_nav_story_list_class(true)
                                                                        } else {
                                                                            story_nav_story_list_class(false)
                                                                        };
                                                                        rsx! {
                                                                            div {
                                                                                class: "{story_list_class}",
                                                                                "aria-hidden": if expanded { "false" } else { "true" },
                                                                                div { class: "tw:grid tw:min-h-0 tw:min-w-0 tw:gap-0.5 tw:overflow-hidden tw:pl-2",
                                                                                    {
                                                                                        let overview_link_class = if component.overview_id == selected {
                                                                                            story_nav_link_class(true, true)
                                                                                        } else {
                                                                                            story_nav_link_class(true, false)
                                                                                        };
                                                                                        let overview_href = story_href(&component.overview_id, selected_viewport);
                                                                                        let overview_id_for_link = component.overview_id.clone();
                                                                                        rsx! {
                                                                                            a {
                                                                                                class: "{overview_link_class}",
                                                                                                href: "{overview_href}",
                                                                                                tabindex: if expanded { "0" } else { "-1" },
                                                                                                onclick: move |_| selected_story_id.set(overview_id_for_link.clone()),
                                                                                                "Overview"
                                                                                            }
                                                                                        }
                                                                                    }
                                                                                    for story in component.stories.iter() {
                                                                                        {
                                                                                            let story_id = story.id;
                                                                                            let link_class = if story.id == selected {
                                                                                                story_nav_link_class(false, true)
                                                                                            } else {
                                                                                                story_nav_link_class(false, false)
                                                                                            };
                                                                                            let story_link = story_href(story_id, selected_viewport);
                                                                                            rsx! {
                                                                                                a {
                                                                                                    class: "{link_class}",
                                                                                                    href: "{story_link}",
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
            section { class: "tw:min-h-0 tw:overflow-y-auto tw:p-[22px]",
                div { class: "tw:mb-4 tw:flex tw:items-start tw:justify-between tw:gap-4",
                    div { class: "tw:grid tw:min-w-0 tw:gap-1",
                        h2 { class: "tw:m-0 tw:text-xl tw:font-bold tw:text-strong-foreground", "{page_title}" }
                        p { class: "tw:m-0 tw:font-mono tw:text-xs tw:text-dim-foreground tw:break-words", "{page_source_ref}" }
                        if !page_description.is_empty() {
                            p { class: "tw:m-0 tw:pt-1.5 tw:text-sm tw:text-dim-foreground", "{page_description}" }
                        }
                    }
                    div { class: "tw:flex tw:gap-2",
                        for target_viewport in [StoryViewport::Sm, StoryViewport::Md, StoryViewport::Lg] {
                            {
                                let selected_for_button = page_id.clone();
                                rsx! {
                                    ViewportButton {
                                        viewport: target_viewport,
                                        active: selected_viewport == target_viewport,
                                        onclick: move |_| {
                                            viewport.set(target_viewport);
                                            set_story_url(&selected_for_button, target_viewport);
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
                {render_story_selection(&selection, selected_viewport)}
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
    overview_id: String,
    stories: Vec<StoryDescriptor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum StorySelection {
    Story(StoryDescriptor),
    ComponentOverview {
        id: String,
        label: String,
        description: String,
        source_path: String,
        stories: Vec<StoryDescriptor>,
    },
}

impl StorySelection {
    fn id(&self) -> &str {
        match self {
            Self::Story(story) => story.id,
            Self::ComponentOverview { id, .. } => id,
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Story(story) => story.label.to_string(),
            Self::ComponentOverview { label, .. } => label.clone(),
        }
    }

    fn description(&self) -> String {
        match self {
            Self::Story(story) => story.description.to_string(),
            Self::ComponentOverview { description, .. } => description.clone(),
        }
    }

    fn source_ref(&self) -> String {
        match self {
            Self::Story(story) => {
                format!("{}:{}", story.source_path, story_function_name(story.story))
            }
            Self::ComponentOverview { source_path, .. } => {
                format!("{source_path}:overview")
            }
        }
    }
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
                    overview_id: component_overview_id(story),
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

/// The first registered story, in book order — the last-resort selection
/// so a stale [`DEFAULT_STORY_ID`] degrades to "wrong story" instead of
/// "no page at all".
fn first_story_selection(groups: &[StoryFamilyGroup]) -> Option<StorySelection> {
    let first = groups
        .iter()
        .flat_map(|family| family.categories.iter())
        .flat_map(|category| category.components.iter())
        .find_map(|component| component.stories.first())?;
    Some(StorySelection::Story(*first))
}

fn story_selection(selected_id: &str, groups: &[StoryFamilyGroup]) -> Option<StorySelection> {
    for family in groups {
        for category in &family.categories {
            for component in &category.components {
                if component.overview_id == selected_id {
                    return Some(StorySelection::ComponentOverview {
                        id: component.overview_id.clone(),
                        label: format!("{} Overview", component.label),
                        description: format!(
                            "All {} stories for this component.",
                            component.stories.len()
                        ),
                        source_path: component_source_path(&component.stories),
                        stories: component.stories.clone(),
                    });
                }

                if let Some(story) = component
                    .stories
                    .iter()
                    .find(|story| story.id == selected_id)
                {
                    return Some(StorySelection::Story(*story));
                }
            }
        }
    }
    None
}

fn story_route_exists(story_id: &str) -> bool {
    if story_by_id(story_id).is_some() {
        return true;
    }

    let stories = all_stories();
    let groups = story_groups(&stories);
    story_selection(story_id, &groups).is_some()
}

fn component_source_path(stories: &[StoryDescriptor]) -> String {
    let Some(first) = stories.first() else {
        return "generated overview".to_string();
    };
    if stories
        .iter()
        .all(|story| story.source_path == first.source_path)
    {
        return first.source_path.to_string();
    }
    "multiple story files".to_string()
}

/// HARNESS SEAM — frozen contract with `scripts/studio-story-pngs.mjs`:
/// a story id ending in [`OVERVIEW_ID_SUFFIX`] is a *generated composite*,
/// never an authored story. The capture script drops those ids from the
/// baseline set, because a composite stacks a whole component's stories into a
/// 10k-25k px page and `captureBeyondViewport` renders composited effects that
/// far below the fold nondeterministically (see
/// `docs/defects/2026-07-28-overview-composite-capture-races.md`). Changing
/// this suffix silently puts the composites back in the pixel pipeline, so
/// `overview_ids_are_reserved_for_generated_composites` pins it.
const OVERVIEW_ID_SUFFIX: &str = "/overview";

fn component_overview_id(story: &StoryDescriptor) -> String {
    let mut id = story.family.to_string();
    id.push('/');
    if let Some(category) = story.category {
        id.push_str(category);
        id.push('/');
    }
    id.push_str(story.component);
    id.push_str(OVERVIEW_ID_SUFFIX);
    id
}

fn story_function_name(story_segment: &str) -> String {
    story_segment.replace('-', "_")
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
    matches!(
        crate::router::current_route(),
        crate::router::StudioRoute::Stories { .. }
    )
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StoryCanvas(
    story_id: &'static str,
    viewport: StoryViewport,
    /// `#[story(screenshot)]`: render the story bare, so the capture is
    /// the app itself — no frame, size label, or checkerboard to crop out
    /// of a README or docs page.
    #[props(default = false)]
    screenshot: bool,
) -> Element {
    let frame_style = viewport.frame_style();
    let canvas_label = viewport.canvas_label();

    if screenshot {
        return rsx! {
            div {
                class: "tw:inline-grid tw:w-max tw:overflow-visible",
                "data-story-capture": "1",
                "data-story-id": "{story_id}",
                div { class: "tw:flow-root tw:min-w-0 tw:overflow-hidden", style: "{frame_style}",
                    {render_story(story_id)}
                }
            }
        };
    }

    rsx! {
        div {
            class: "tw:inline-grid tw:w-max tw:overflow-visible",
            "data-story-capture": "1",
            "data-story-id": "{story_id}",
            div { class: "tw:box-content tw:flow-root tw:min-w-0 tw:overflow-hidden tw:rounded-sm tw:border-4 tw:border-border-muted tw:bg-card-muted", style: "{frame_style}",
                div { class: "tw:flex tw:min-w-0 tw:w-full tw:justify-start tw:border-b-4 tw:border-border-muted tw:bg-border-muted",
                    span { class: "tw:px-2 tw:py-1 tw:font-mono tw:text-xs tw:leading-none tw:text-subtle-foreground", "{canvas_label}" }
                }
                div { class: "tw:flow-root tw:min-w-0 tw:w-full tw:bg-card-muted tw:p-2",
                    div { class: "story-frame-checker tw:flow-root tw:min-w-0 tw:w-full",
                        {render_story(story_id)}
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StoryFrame(story_id: &'static str, viewport: StoryViewport) -> Element {
    let frame_style = viewport.frame_style();
    let canvas_label = viewport.canvas_label();

    rsx! {
        div { class: "tw:inline-grid tw:w-max tw:overflow-visible",
            div { class: "tw:box-content tw:flow-root tw:min-w-0 tw:overflow-hidden tw:rounded-sm tw:border-4 tw:border-border-muted tw:bg-card-muted", style: "{frame_style}",
                div { class: "tw:flex tw:min-w-0 tw:w-full tw:justify-start tw:border-b-4 tw:border-border-muted tw:bg-border-muted",
                    span { class: "tw:px-2 tw:py-1 tw:font-mono tw:text-xs tw:leading-none tw:text-subtle-foreground", "{canvas_label}" }
                }
                div { class: "tw:flow-root tw:min-w-0 tw:w-full tw:bg-card-muted tw:p-2",
                    div { class: "story-frame-checker tw:flow-root tw:min-w-0 tw:w-full",
                        {render_story(story_id)}
                    }
                }
            }
        }
    }
}

fn render_story_selection(selection: &StorySelection, viewport: StoryViewport) -> Element {
    match selection {
        StorySelection::Story(story) => rsx! {
            StoryCanvas {
                key: "{story.id}",
                story_id: story.id,
                viewport,
                screenshot: story.screenshot,
            }
        },
        StorySelection::ComponentOverview { id, stories, .. } => rsx! {
            StoryOverviewCanvas {
                key: "{id}",
                story_id: id.clone(),
                stories: stories.clone(),
                viewport,
            }
        },
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StoryOverviewCanvas(
    story_id: String,
    stories: Vec<StoryDescriptor>,
    viewport: StoryViewport,
) -> Element {
    rsx! {
        div { class: "tw:grid tw:w-max tw:gap-[26px]",
            "data-story-capture": "1",
            "data-story-id": "{story_id}",
            for story in stories {
                section {
                    key: "{story.id}",
                    class: "tw:grid tw:w-max tw:gap-2.5 tw:border-b tw:border-border-muted tw:pb-6 tw:last:border-b-0 tw:last:pb-0",
                    header { class: "tw:grid tw:min-w-0 tw:gap-1",
                        h3 { class: "tw:m-0 tw:text-base tw:font-bold tw:text-strong-foreground", "{story.label}" }
                        p { class: "tw:m-0 tw:font-mono tw:text-xs tw:text-dim-foreground tw:break-words", "{story.source_path}" }
                    }
                    div { class: "tw:min-w-0",
                        StoryFrame {
                            key: "{story.id}",
                            story_id: story.id,
                            viewport,
                        }
                    }
                }
            }
        }
    }
}

fn story_nav_component_class(active: bool) -> &'static str {
    if active {
        "tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-raised tw:px-2 tw:py-1.5 tw:text-sm tw:leading-tight tw:text-strong-foreground tw:no-underline"
    } else {
        "tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:rounded-sm tw:border tw:border-transparent tw:px-2 tw:py-1.5 tw:text-sm tw:leading-tight tw:text-soft-foreground tw:no-underline tw:hover:bg-card-raised tw:hover:text-strong-foreground"
    }
}

fn story_nav_story_list_class(expanded: bool) -> &'static str {
    if expanded {
        "tw:grid tw:grid-rows-[1fr] tw:opacity-100 tw:transition-[grid-template-rows,opacity] tw:duration-150"
    } else {
        "tw:grid tw:grid-rows-[0fr] tw:opacity-0 tw:transition-[grid-template-rows,opacity] tw:duration-150"
    }
}

/// The active entry wears the you-are-here side line (`ux-here-line-y`,
/// cool sweep) — the selection grammar's vertical nav mark — over the
/// selection-bg fade. The transparent `border-l-2` stays for layout so
/// active and inactive entries share metrics.
fn story_nav_link_class(overview: bool, active: bool) -> &'static str {
    match (overview, active) {
        (true, true) => {
            "tw:block tw:min-w-0 ux-here-line-y tw:border-l-2 tw:border-transparent tw:bg-[linear-gradient(90deg,var(--studio-color-selection-bg),transparent_90%)] tw:py-1 tw:pl-2.5 tw:text-sm tw:font-extrabold tw:leading-tight tw:text-strong-foreground tw:no-underline tw:break-words"
        }
        (true, false) => {
            "tw:block tw:min-w-0 tw:border-l-2 tw:border-transparent tw:py-1 tw:pl-2.5 tw:text-sm tw:font-extrabold tw:leading-tight tw:text-soft-foreground tw:no-underline tw:break-words tw:hover:text-strong-foreground"
        }
        (false, true) => {
            "tw:block tw:min-w-0 ux-here-line-y tw:border-l-2 tw:border-transparent tw:bg-[linear-gradient(90deg,var(--studio-color-selection-bg),transparent_90%)] tw:py-1 tw:pl-2.5 tw:text-sm tw:leading-tight tw:text-strong-foreground tw:no-underline tw:break-words"
        }
        (false, false) => {
            "tw:block tw:min-w-0 tw:border-l-2 tw:border-transparent tw:py-1 tw:pl-2.5 tw:text-sm tw:leading-tight tw:text-muted-foreground tw:no-underline tw:break-words tw:hover:text-strong-foreground"
        }
    }
}

fn viewport_button_class(active: bool) -> &'static str {
    if active {
        "tw:grid tw:min-w-[58px] tw:gap-px tw:rounded-sm tw:border tw:border-selection-border tw:bg-card-raised tw:px-2.5 tw:py-1.5 tw:text-left tw:leading-tight"
    } else {
        "tw:grid tw:min-w-[58px] tw:gap-px tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-raised tw:px-2.5 tw:py-1.5 tw:text-left tw:leading-tight"
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
        viewport_button_class(true)
    } else {
        viewport_button_class(false)
    };
    rsx! {
        button {
            class,
            type: "button",
            onclick: move |event| onclick.call(event),
            span { class: "tw:text-xs tw:font-extrabold tw:text-strong-foreground", "{viewport.slug()}" }
            span { class: "tw:text-[0.66rem] tw:text-subtle-foreground", "{viewport.width_label()}" }
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
            Self::Sm => "width: 390px;",
            Self::Md => "width: 720px;",
            Self::Lg => "width: 1080px;",
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
            Self::Sm => "390px",
            Self::Md => "720px",
            Self::Lg => "1080px",
        }
    }

    const fn canvas_label(self) -> &'static str {
        match self {
            Self::Sm => "sm - 390px",
            Self::Md => "md - 720px",
            Self::Lg => "lg - 1080px",
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

fn selected_story_route_from_url() -> StoryRoute {
    // route identity comes from the shared router vocabulary; the
    // `?viewport=` query stays this module's concern
    let story_id = match crate::router::current_route() {
        crate::router::StudioRoute::Stories { story_id: Some(id) } if story_route_exists(&id) => id,
        _ => DEFAULT_STORY_ID.to_string(),
    };
    StoryRoute {
        story_id,
        viewport: viewport_from_query(),
    }
}

fn viewport_from_query() -> StoryViewport {
    location_search()
        .unwrap_or_default()
        .trim_start_matches('?')
        .split('&')
        .filter_map(|part| part.split_once('='))
        .find_map(|(key, value)| {
            (key == "viewport")
                .then(|| StoryViewport::parse(value))
                .flatten()
        })
        .unwrap_or(StoryViewport::Lg)
}

/// One story's address: the shared `/stories/<id>` path plus this module's
/// own `?viewport=` param.
fn story_href(story_id: &str, viewport: StoryViewport) -> String {
    let route = story_route(story_id);
    format!("{}?viewport={}", route.path(), viewport.slug())
}

fn story_route(story_id: &str) -> crate::router::StudioRoute {
    crate::router::StudioRoute::Stories {
        story_id: Some(story_id.to_string()),
    }
}

/// The viewport buttons set their signal themselves, so the URL write is a
/// silent `replaceState` (a zoom on the story you are already reading is not
/// a new history entry) that keeps every other param — the capture harness's
/// `?story-png=` included.
fn set_story_url(story_id: &str, viewport: StoryViewport) {
    crate::router::replace_with_query_param(&story_route(story_id), "viewport", viewport.slug());
}

/// Browser navigation inside the book: back/forward and the sidebar's plain
/// `<a href="/stories/…">` links, both through the shared router listener
/// (the book mounts before the app's hooks, so it installs its own).
fn install_story_route_listener(
    mut selected_story_id: Signal<String>,
    mut viewport: Signal<StoryViewport>,
) -> Option<std::rc::Rc<crate::router::RouteListener>> {
    crate::router::install_route_listener(move |event| {
        // The book has no session to guard, so every click intent is
        // permitted and only the completed move is acted on.
        if matches!(event, crate::router::NavEvent::ClickIntent(_)) {
            return true;
        }
        let route = selected_story_route_from_url();
        selected_story_id.set(route.story_id);
        viewport.set(route.viewport);
        true
    })
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

/// HARNESS SEAM — frozen contract with `scripts/studio-story-pngs.mjs`:
/// capture URLs are `?story-png=1&story=<id>&viewport=<vp>#/stories/<id>`.
/// These query params are not routing (see `crate::router`) and must not
/// change shape without updating the capture script in the same commit.
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

fn location_search() -> Option<String> {
    web_sys::window()
        .map(|window| window.location())
        .and_then(|location| location.search().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The capture script (`scripts/studio-story-pngs.mjs`) tells generated
    /// composites apart from authored stories by the `/overview` id suffix and
    /// keeps the composites out of the committed baseline set. That only holds
    /// while the suffix is exclusively ours: a `#[story] fn overview()` would
    /// both collide with its own component's composite in the nav and quietly
    /// lose its pixel baseline.
    #[test]
    fn overview_ids_are_reserved_for_generated_composites() {
        let stories = all_stories();
        assert!(!stories.is_empty(), "story registry is empty");

        let authored: Vec<&str> = stories
            .iter()
            .map(|story| story.id)
            .filter(|id| id.ends_with(OVERVIEW_ID_SUFFIX))
            .collect();
        assert!(
            authored.is_empty(),
            "authored stories may not end in `{OVERVIEW_ID_SUFFIX}` — that suffix marks the \
             generated composites the capture script excludes from baselines: {authored:?}",
        );

        for story in &stories {
            let overview_id = component_overview_id(story);
            assert!(
                overview_id.ends_with(OVERVIEW_ID_SUFFIX),
                "generated overview id `{overview_id}` must end in `{OVERVIEW_ID_SUFFIX}`",
            );
        }
    }
}
