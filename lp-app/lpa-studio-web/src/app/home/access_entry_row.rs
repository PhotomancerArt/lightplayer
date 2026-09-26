//! One row of "Who has access" (spike §2): who, what it can do, and its
//! trash can.
//!
//! The icon says what kind of holder it is — a laptop or a phone for a
//! browser (guessed from the label; only the icon rides on the guess), the
//! account's initial in a circle, a key for a password. Only a play entry
//! says what it can do ("can play" and a PLAY chip): edit is what a key
//! normally is. The trash can is the studio's two-tap confirm: the first
//! tap arms it ("Remove", red, the 4 s drain, the row dimmed), the second
//! removes.

use dioxus::prelude::*;
use lpa_studio_core::{
    AccessCommand, AccessTier, DeviceAccessChange, DeviceId, SecretKind, UiAccessEntry,
};

use super::access_fields::PLAY_CHIP_CLASS;
use super::browser_identity::label_looks_like_phone;
use crate::base::{StudioIcon, StudioIconName};
use crate::core::ArmedConfirmButton;

/// See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AccessEntryRow(
    entry: UiAccessEntry,
    device: DeviceId,
    #[props(default)] busy: bool,
    /// Stories only: the trash can starts armed.
    #[props(default)]
    armed_preview: bool,
    on_access: EventHandler<AccessCommand>,
) -> Element {
    let salt = entry.salt_id;
    let detail = entry_detail(&entry);
    let play = entry.tier == AccessTier::Play;
    rsx! {
        li { class: "ux-armed-row tw:flex tw:min-w-0 tw:items-center tw:gap-2.5 tw:border-t tw:border-border-muted tw:py-2 tw:first:border-t-0",
            span { class: "ux-armed-row-dim tw:flex-none", EntryIcon { entry: entry.clone() } }
            span { class: "ux-armed-row-dim tw:grid tw:min-w-0 tw:flex-1 tw:gap-px",
                span { class: "tw:flex tw:min-w-0 tw:items-baseline tw:gap-1.5",
                    span { class: "tw:min-w-0 tw:truncate tw:text-[13px] tw:font-bold tw:text-strong-foreground",
                        title: "{entry.label}",
                        "{entry.label}"
                    }
                    if entry.is_this_browser {
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
                title: format!("Remove {}", entry.label),
                disabled: busy,
                armed_preview,
                on_confirm: move |_| on_access.call(AccessCommand::Change {
                    device,
                    change: DeviceAccessChange::Remove { salt },
                }),
            }
        }
    }
}

/// The row's icon tile.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn EntryIcon(entry: UiAccessEntry) -> Element {
    match entry_icon(&entry) {
        EntryGlyph::Initial(initial) => rsx! {
            span { class: "{ICON_TILE_CLASS} tw:rounded-full tw:border-status-good-border tw:bg-status-good-bg tw:text-xs tw:font-extrabold tw:text-status-good-foreground",
                "{initial}"
            }
        },
        EntryGlyph::Password => rsx! {
            span { class: "{ICON_TILE_CLASS} tw:border-status-warning-border tw:bg-status-warning-bg tw:text-status-warning-foreground",
                StudioIcon { name: StudioIconName::AccessKey, size: 15 }
            }
        },
        EntryGlyph::Icon(name) => rsx! {
            span { class: "{ICON_TILE_CLASS} tw:border-status-neutral-border tw:bg-status-neutral-bg tw:text-status-neutral-foreground",
                StudioIcon { name, size: 15 }
            }
        },
    }
}

/// A 30px rounded tile.
pub(crate) const ICON_TILE_CLASS: &str = "tw:inline-flex tw:h-[30px] tw:w-[30px] tw:flex-none tw:items-center tw:justify-center tw:rounded-lg tw:border";

/// What an entry's icon tile shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EntryGlyph {
    Icon(StudioIconName),
    /// An account: its name's first letter.
    Initial(String),
    Password,
}

pub(crate) fn entry_icon(entry: &UiAccessEntry) -> EntryGlyph {
    match entry.kind {
        SecretKind::Browser if label_looks_like_phone(&entry.label) => {
            EntryGlyph::Icon(StudioIconName::AccessPhone)
        }
        SecretKind::Browser => EntryGlyph::Icon(StudioIconName::AccessLaptop),
        SecretKind::Account => EntryGlyph::Initial(
            entry
                .label
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "?".to_string()),
        ),
        SecretKind::Password => EntryGlyph::Password,
    }
}

/// The order the list reads in: this browser, other browsers, accounts,
/// your account's passwords, shared passwords. Stable within a group (the
/// board's own order).
pub(crate) fn ordered(entries: &[UiAccessEntry]) -> Vec<UiAccessEntry> {
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(
        |entry| match (entry.is_this_browser, entry.kind, entry.is_account) {
            (true, _, _) => 0,
            (false, SecretKind::Browser, _) => 1,
            (false, SecretKind::Account, _) => 2,
            (false, SecretKind::Password, true) => 3,
            (false, SecretKind::Password, false) => 4,
        },
    );
    sorted
}

/// The row's second line: "can play · " for a play entry, then where it
/// came from.
pub(crate) fn entry_detail(entry: &UiAccessEntry) -> String {
    let added = entry
        .added_at
        .map(|secs| format!("added {}", short_date(secs)));
    let origin = match (entry.kind, entry.is_account) {
        (SecretKind::Account, true) => "any browser signed in as you".to_string(),
        (SecretKind::Password, true) => "from your account".to_string(),
        (SecretKind::Password, false) => match added {
            Some(added) => format!("password · {added}"),
            None => "password".to_string(),
        },
        (SecretKind::Browser | SecretKind::Account, _) => {
            added.unwrap_or_else(|| "added by USB".to_string())
        }
    };
    match entry.tier {
        AccessTier::Play => format!("can play · {origin}"),
        AccessTier::Edit => origin,
    }
}

/// "Sep 24" from epoch seconds (UTC: a day either way is fine here).
pub(crate) fn short_date(epoch_secs: u64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    // Days since 1970-01-01 to a civil date (the proleptic Gregorian
    // calendar, counted in 400-year eras from 0000-03-01).
    let days = (epoch_secs / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    format!("{} {day}", MONTHS[(month - 1) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_reads_this_browser_first_then_by_kind() {
        let list = ordered(&[
            entry("friends", SecretKind::Password, false, false),
            entry("Yona's play password", SecretKind::Password, true, false),
            entry("Yona's account", SecretKind::Account, true, false),
            entry("Yona's iPhone", SecretKind::Browser, false, false),
            entry("Yona's Mac", SecretKind::Browser, false, true),
        ]);
        let labels: Vec<&str> = list.iter().map(|entry| entry.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Yona's Mac",
                "Yona's iPhone",
                "Yona's account",
                "Yona's play password",
                "friends"
            ]
        );
    }

    #[test]
    fn only_play_says_what_it_can_do() {
        let mut friends = entry("friends", SecretKind::Password, false, false);
        friends.tier = AccessTier::Play;
        friends.added_at = Some(1_790_000_000);
        assert_eq!(entry_detail(&friends), "can play · password · added Sep 21");
        let account = entry("Yona's account", SecretKind::Account, true, false);
        assert_eq!(entry_detail(&account), "any browser signed in as you");
    }

    #[test]
    fn icons_follow_the_kind() {
        assert_eq!(
            entry_icon(&entry("Yona's iPhone", SecretKind::Browser, false, false)),
            EntryGlyph::Icon(StudioIconName::AccessPhone)
        );
        assert_eq!(
            entry_icon(&entry("Chrome on Mac", SecretKind::Browser, false, false)),
            EntryGlyph::Icon(StudioIconName::AccessLaptop)
        );
        assert_eq!(
            entry_icon(&entry("sam's account", SecretKind::Account, false, false)),
            EntryGlyph::Initial("S".to_string())
        );
    }

    #[test]
    fn dates_are_month_and_day() {
        assert_eq!(short_date(0), "Jan 1");
        // 2026-09-24 12:00 UTC.
        assert_eq!(short_date(1_790_251_200), "Sep 24");
        // 2024-02-29 (a leap day).
        assert_eq!(short_date(1_709_164_800), "Feb 29");
    }

    fn entry(
        label: &str,
        kind: SecretKind,
        is_account: bool,
        is_this_browser: bool,
    ) -> UiAccessEntry {
        UiAccessEntry {
            label: label.to_string(),
            kind,
            tier: AccessTier::Edit,
            salt_id: [label.len() as u8; 16],
            is_this_browser,
            is_account,
            added_at: None,
        }
    }
}
