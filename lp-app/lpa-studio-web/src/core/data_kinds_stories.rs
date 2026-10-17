//! "Data kinds": every kind of data Studio names, and every case of each,
//! in the one rule core keeps for it (`lpa_studio_core::core::data_kinds`,
//! P17 in `docs/ux/README.md`). One section per kind; the left column is
//! the value as it arrives, the right what every surface says.

use dioxus::prelude::*;
use lpa_studio_core::{
    CalendarDate, PRE_HELLO_FIRMWARE, age_spoken, age_words, byte_size, date_words,
    duration_words, firmware_label_version, firmware_version, percent, pre_ota_version,
};
use lpa_studio_web_story_macros::story;

/// One day, in seconds.
const DAY: f64 = 86_400.0;
/// 2026-10-16 12:00 UTC, the day round 2 of the card's words shipped.
const OCT_16_2026: f64 = 1_792_152_000.0;

#[story(
    description = "Every data kind Studio shows and every case of it, as core says it: ages, durations, an age in a sentence (the board card's surface and its Link row), byte sizes, percentages, dates and firmware versions."
)]
fn every_kind() -> Element {
    let ages: Vec<(String, String)> = [0.0, 12.0, 89.0, 90.0, 2.0 * 3_600.0, 3.0 * DAY, 14.0 * DAY]
        .into_iter()
        .map(|secs| (seconds(secs), age_words(secs)))
        .collect();
    let durations: Vec<(String, String)> = [0.0, 89.0, 90.0, 2.0 * 3_600.0, 14.0 * DAY]
        .into_iter()
        .map(|secs| (seconds(secs), duration_words(secs)))
        .collect();
    let spoken: Vec<(String, String)> = [0.0, 1.0, 12.0, 90.0, 2.0 * 3_600.0, 3.0 * DAY, 14.0 * DAY]
        .into_iter()
        .map(|secs| (seconds(secs), age_spoken(secs)))
        .collect();
    let sizes: Vec<(String, String)> = [
        0_u64,
        512,
        1_536,
        4_096,
        38 * 1_024,
        720_896,
        4_404_019,
        64 * 1_024 * 1_024,
    ]
    .into_iter()
    .map(|bytes| (format!("{bytes} B"), byte_size(bytes)))
    .collect();
    let percents: Vec<(String, String)> = [0_u32, 42, 100, 250]
        .into_iter()
        .map(|value| (value.to_string(), percent(value)))
        .collect();
    let dates: Vec<(String, String)> = vec![
        ("2026-10-16, epoch".to_string(), date_words(OCT_16_2026)),
        ("1970-01-01, epoch".to_string(), date_words(0.0)),
        (
            "2026-10-07, a release's".to_string(),
            CalendarDate::parse_iso("2026-10-07")
                .map(|date| date.words())
                .unwrap_or_default(),
        ),
        (
            "2026-08-06, where the year is the fact".to_string(),
            CalendarDate::from_epoch_secs(1_785_974_400.0).words_with_year(),
        ),
    ];
    let versions: Vec<(String, String)> = vec![
        (
            "release 2026.10.16-1".to_string(),
            firmware_version("2026.10.16-1"),
        ),
        (
            "hello label fw-esp32c6 2026.10.05-2".to_string(),
            firmware_label_version("fw-esp32c6 2026.10.05-2"),
        ),
        (
            "pre-OTA core 2026.10.02-1".to_string(),
            pre_ota_version("2026.10.02-1"),
        ),
        ("dev c688c990b".to_string(), firmware_version("c688c990b")),
        (
            "dev 5eb70a7-dirty-142233PT".to_string(),
            firmware_version("5eb70a7-dirty-142233PT"),
        ),
        (
            "pre-hello board".to_string(),
            PRE_HELLO_FIRMWARE.to_string(),
        ),
        ("nothing said".to_string(), firmware_version("")),
    ];
    rsx! {
        div { class: "tw:grid tw:max-w-[560px] tw:gap-5 tw:p-4",
            KindSection { title: "Age", rule: "a reading: how long ago", rows: ages }
            KindSection { title: "Duration", rule: "for how long: 2 h", rows: durations }
            KindSection { title: "Age in a sentence", rule: "spelled out: It was last seen 2 hours ago.", rows: spoken }
            KindSection { title: "Byte size", rule: "1 KB = 1024 B; one decimal below ten", rows: sizes }
            KindSection { title: "Percent", rule: "a whole number, never past 100", rows: percents }
            KindSection { title: "Date", rule: "UTC, the month's short name and the day", rows: dates }
            KindSection { title: "Firmware version", rule: "a release as its tag; never the target", rows: versions }
        }
    }
}

/// One kind: its name, its rule, and every case as it arrives and as it
/// reads.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn KindSection(title: &'static str, rule: &'static str, rows: Vec<(String, String)>) -> Element {
    rsx! {
        section { class: "tw:grid tw:gap-1.5", "data-kind": "{title}",
            div { class: "tw:flex tw:items-baseline tw:gap-2",
                h3 { class: "tw:m-0 tw:text-[13px] tw:font-bold tw:text-strong-foreground", "{title}" }
                span { class: "tw:text-[11.5px] tw:text-subtle-foreground", "{rule}" }
            }
            div { class: "tw:grid tw:grid-cols-[1fr_auto] tw:gap-x-6 tw:gap-y-0.5 tw:rounded-md tw:border tw:border-solid tw:border-border tw:bg-card tw:px-3 tw:py-2",
                for (given, said) in rows {
                    span { class: "tw:font-mono tw:text-[11px] tw:text-subtle-foreground", "{given}" }
                    span { class: "tw:text-[12px] tw:text-foreground", "{said}" }
                }
            }
        }
    }
}

/// An age's input, as a reader would write it.
fn seconds(secs: f64) -> String {
    format!("{secs:.0} s")
}
