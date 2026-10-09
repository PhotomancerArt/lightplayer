//! The home page's tab strip: All · Boards · Projects · Patterns, in the
//! underline look of the top bar's tabs (the spike's `.c-tab`), with a small
//! count after the label.
//!
//! A tab is view state (PD4): selecting one writes a signal the page owns
//! and presses no action. Which sections a tab shows is core's table
//! (`UiHomeTab::shows`); this strip only names the tabs and their counts.

use dioxus::prelude::*;
use lpa_studio_core::{UiHomeSections, UiHomeTab};

/// One tab of the strip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FilterTab {
    /// Stable lowercase key (what `on_select` hands back).
    pub key: &'static str,
    pub label: &'static str,
    /// A small number after the label; `None` draws none.
    pub count: Option<usize>,
}

/// The strip. `selected` is the key of the tab that is on.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn FilterTabs(
    tabs: Vec<FilterTab>,
    selected: String,
    on_select: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            class: "tw:flex tw:min-w-0 tw:flex-1 tw:gap-[18px] tw:overflow-x-auto tw:[scrollbar-width:none]",
            role: "tablist",
            for tab in tabs {
                {
                    let on = tab.key == selected;
                    let key = tab.key;
                    rsx! {
                        button {
                            key: "{tab.key}",
                            class: tab_class(on),
                            r#type: "button",
                            role: "tab",
                            "aria-selected": "{on}",
                            onclick: move |_| on_select.call(key.to_string()),
                            "{tab.label}"
                            if let Some(count) = tab.count {
                                span { class: COUNT_CLASS, "{count}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The page's four tabs for these sections: the label from core, the count
/// from [`UiHomeSections::count`].
pub(crate) fn home_filter_tabs(sections: &UiHomeSections) -> Vec<FilterTab> {
    UiHomeTab::ALL
        .into_iter()
        .map(|tab| FilterTab {
            key: tab.key(),
            label: tab.label(),
            count: sections.count(tab),
        })
        .collect()
}

/// The tab a strip key names; an unknown key reads as All.
pub(crate) fn home_tab_for_key(key: &str) -> UiHomeTab {
    UiHomeTab::from_key(key).unwrap_or_default()
}

/// A tab's classes: the underline, in the spectrum, under the one that is on.
fn tab_class(on: bool) -> &'static str {
    if on {
        "tw:relative tw:inline-flex tw:cursor-pointer tw:items-center tw:gap-1.5 tw:whitespace-nowrap tw:border-0 tw:bg-transparent tw:px-0 tw:pt-2 tw:pb-[9px] tw:text-[12.5px] tw:font-semibold tw:text-strong-foreground tw:after:absolute tw:after:inset-x-0 tw:after:-bottom-px tw:after:h-0.5 tw:after:rounded-full tw:after:bg-[linear-gradient(90deg,var(--studio-spectrum))] tw:after:content-[''] ux-focus-ring"
    } else {
        "tw:relative tw:inline-flex tw:cursor-pointer tw:items-center tw:gap-1.5 tw:whitespace-nowrap tw:border-0 tw:bg-transparent tw:px-0 tw:pt-2 tw:pb-[9px] tw:text-[12.5px] tw:font-semibold tw:text-muted-foreground tw:hover:text-strong-foreground ux-focus-ring"
    }
}

/// The small number after a label.
const COUNT_CLASS: &str = "tw:text-[11px] tw:font-semibold tw:text-dim-foreground";

#[cfg(test)]
mod tests {
    use lpa_studio_core::{DeviceId, UiHomeBoard, UiHomeBoardKind};

    use super::*;

    #[test]
    fn the_strip_has_the_four_tabs_with_their_counts() {
        let board = |id: u64, kind| UiHomeBoard {
            id: DeviceId(id),
            kind,
            title: format!("board {id}"),
            status: "Ready".to_string(),
            project: None,
            row_verbs: Vec::new(),
        };
        let sections = UiHomeSections {
            online: vec![board(1, UiHomeBoardKind::Connected)],
            offline: vec![board(2, UiHomeBoardKind::Remembered)],
            projects: vec!["prja".to_string(), "prjb".to_string(), "prjc".to_string()],
            patterns: vec!["prjp".to_string()],
            ..UiHomeSections::default()
        };
        let tabs = home_filter_tabs(&sections);
        let shown: Vec<(&str, &str, Option<usize>)> = tabs
            .iter()
            .map(|tab| (tab.key, tab.label, tab.count))
            .collect();
        assert_eq!(
            shown,
            [
                ("all", "All", None),
                ("boards", "Boards", Some(2)),
                ("projects", "Projects", Some(3)),
                ("patterns", "Patterns", Some(1)),
            ]
        );
    }

    #[test]
    fn every_key_maps_back_to_its_tab_and_a_stranger_reads_as_all() {
        for tab in UiHomeTab::ALL {
            assert_eq!(home_tab_for_key(tab.key()), tab);
        }
        assert_eq!(home_tab_for_key("devices"), UiHomeTab::All);
        assert_eq!(home_tab_for_key(""), UiHomeTab::All);
    }
}
