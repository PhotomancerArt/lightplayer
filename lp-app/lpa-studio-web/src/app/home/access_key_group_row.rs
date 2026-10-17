//! One row of "Your browsers & account" (spike 4B): who, how many, since
//! when, and a trash can for the lot.
//!
//! The icon says what kind of holder it is — a laptop or a phone for a
//! browser (guessed from the label; only the icon rides on the guess), the
//! account's initial in a circle, a key for a password. A group of more
//! than one reads "Brave on Mac ×11" over its date span. Only a play entry
//! says what it can do (the PLAY chip): author is what a key normally is.
//! The trash can is the studio's two-tap confirm: the first tap arms it
//! ("Remove", red, the 4 s drain, the row dimmed), the second removes every
//! entry in the group.

use dioxus::prelude::*;
use lpa_studio_core::{
    AccessCommand, AccessTier, DeviceAccessChange, DeviceId, SecretKind, UiKeyGroup,
};

use super::access_fields::PLAY_CHIP_CLASS;
use super::browser_identity::label_looks_like_phone;
use crate::base::{StudioIcon, StudioIconName};
use crate::core::ArmedConfirmButton;

/// See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AccessKeyGroupRow(
    group: UiKeyGroup,
    device: DeviceId,
    #[props(default)] busy: bool,
    /// Stories only: the trash can starts armed.
    #[props(default)]
    armed_preview: bool,
    on_access: EventHandler<AccessCommand>,
) -> Element {
    let salts = group.salts.clone();
    let detail = group_detail(&group);
    let play = group.tier == AccessTier::Play;
    let count = group.count();
    let title = match count {
        1 => format!("Remove {}", group.label),
        n => format!("Remove all {n} {}", group.label),
    };
    rsx! {
        li { class: "ux-armed-row tw:flex tw:min-w-0 tw:items-center tw:gap-2.5 tw:border-t tw:border-border-muted tw:py-1.5 tw:first:border-t-0",
            span { class: "ux-armed-row-dim tw:flex-none", GroupIcon { group: group.clone() } }
            span { class: "ux-armed-row-dim tw:grid tw:min-w-0 tw:flex-1 tw:gap-px",
                span { class: "tw:flex tw:min-w-0 tw:items-baseline tw:gap-1.5",
                    span { class: "tw:min-w-0 tw:truncate tw:text-[13px] tw:font-bold tw:text-strong-foreground",
                        title: "{group.label}",
                        "{group.label}"
                    }
                    if count > 1 {
                        span { class: "tw:flex-none tw:font-mono tw:text-[11px] tw:font-bold tw:text-dim-foreground", "×{count}" }
                    }
                    if group.is_this_browser {
                        span { class: "tw:flex-none tw:font-mono tw:text-[9.5px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-status-good-foreground",
                            "this browser"
                        }
                    }
                }
                span { class: "ux-armed-row-hide tw:truncate tw:text-[11px] tw:text-dim-foreground", "{detail}" }
            }
            if play {
                span { class: "ux-armed-row-dim {PLAY_CHIP_CLASS}", "play" }
            }
            ArmedConfirmButton {
                icon: Some(StudioIconName::Remove),
                armed_label: "Remove".to_string(),
                title,
                disabled: busy,
                armed_preview,
                on_confirm: move |_| on_access.call(AccessCommand::Change {
                    device,
                    change: DeviceAccessChange::Remove { salts: salts.clone() },
                }),
            }
        }
    }
}

/// The row's icon tile.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn GroupIcon(group: UiKeyGroup) -> Element {
    match group_icon(&group) {
        GroupGlyph::Initial(initial) => rsx! {
            span { class: "{ICON_TILE_CLASS} tw:rounded-full tw:border-status-good-border tw:bg-status-good-bg tw:text-xs tw:font-extrabold tw:text-status-good-foreground",
                "{initial}"
            }
        },
        GroupGlyph::Password => rsx! {
            span { class: "{ICON_TILE_CLASS} tw:border-status-warning-border tw:bg-status-warning-bg tw:text-status-warning-foreground",
                StudioIcon { name: StudioIconName::AccessKey, size: 15 }
            }
        },
        GroupGlyph::Icon(name) => rsx! {
            span { class: "{ICON_TILE_CLASS} tw:border-status-neutral-border tw:bg-status-neutral-bg tw:text-status-neutral-foreground",
                StudioIcon { name, size: 15 }
            }
        },
    }
}

/// A 30px rounded tile.
pub(crate) const ICON_TILE_CLASS: &str = "tw:inline-flex tw:h-[30px] tw:w-[30px] tw:flex-none tw:items-center tw:justify-center tw:rounded-lg tw:border";

/// What a group's icon tile shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GroupGlyph {
    Icon(StudioIconName),
    /// An account: its name's first letter.
    Initial(String),
    Password,
}

pub(crate) fn group_icon(group: &UiKeyGroup) -> GroupGlyph {
    match group.kind {
        SecretKind::Browser if label_looks_like_phone(&group.label) => {
            GroupGlyph::Icon(StudioIconName::AccessPhone)
        }
        SecretKind::Browser => GroupGlyph::Icon(StudioIconName::AccessLaptop),
        SecretKind::Account => GroupGlyph::Initial(
            group
                .label
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "?".to_string()),
        ),
        SecretKind::Password => GroupGlyph::Password,
    }
}

/// The row's second line: where it came from, and when.
pub(crate) fn group_detail(group: &UiKeyGroup) -> String {
    let when = match (group.first_added, group.last_added) {
        (Some(first), Some(last)) if short_date(first) != short_date(last) => {
            Some(format!("{} – {}", short_date(first), short_date(last)))
        }
        (Some(first), _) => Some(format!("added {}", short_date(first))),
        _ => None,
    };
    match (group.kind, group.is_account) {
        (SecretKind::Account, true) => "any browser signed in as you".to_string(),
        (SecretKind::Password, true) => "from your account".to_string(),
        _ => when.unwrap_or_else(|| "added by USB".to_string()),
    }
}

/// A key's day, in Studio's one date format ("Sep 24", UTC: a day either
/// way is fine here).
fn short_date(epoch_secs: u64) -> String {
    lpa_studio_core::date_words(epoch_secs as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_reads_its_date_span() {
        let mut brave = group("Brave on Mac", SecretKind::Browser, false);
        brave.salts = vec![[1; 16], [2; 16]];
        brave.first_added = Some(1_790_251_200);
        brave.last_added = Some(1_790_251_200 + 8 * 86_400);
        assert_eq!(group_detail(&brave), "Sep 24 – Oct 2");
        brave.last_added = brave.first_added;
        assert_eq!(group_detail(&brave), "added Sep 24");
        let account = group("Yona's account", SecretKind::Account, true);
        assert_eq!(group_detail(&account), "any browser signed in as you");
        assert_eq!(
            group_detail(&group("Chrome on Mac", SecretKind::Browser, false)),
            "added by USB"
        );
    }

    #[test]
    fn icons_follow_the_kind() {
        assert_eq!(
            group_icon(&group("Yona's iPhone", SecretKind::Browser, false)),
            GroupGlyph::Icon(StudioIconName::AccessPhone)
        );
        assert_eq!(
            group_icon(&group("Chrome on Mac", SecretKind::Browser, false)),
            GroupGlyph::Icon(StudioIconName::AccessLaptop)
        );
        assert_eq!(
            group_icon(&group("sam's account", SecretKind::Account, false)),
            GroupGlyph::Initial("S".to_string())
        );
    }

    fn group(label: &str, kind: SecretKind, is_account: bool) -> UiKeyGroup {
        UiKeyGroup {
            label: label.to_string(),
            kind,
            tier: AccessTier::Edit,
            salts: vec![[label.len() as u8; 16]],
            is_this_browser: false,
            is_account,
            first_added: None,
            last_added: None,
        }
    }
}
