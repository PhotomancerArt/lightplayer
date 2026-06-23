use dioxus::prelude::*;
use dioxus_icons::lucide::{CircleAlert, CircleMinus, FlaskConical, Play, Usb};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StudioIconName {
    Play,
    Usb,
    Test,
    StatusRunning,
    StatusIdle,
    StatusError,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioIcon(name: StudioIconName, size: u32) -> Element {
    match name {
        StudioIconName::Play => rsx! { Play { size } },
        StudioIconName::Usb => rsx! { Usb { size } },
        StudioIconName::Test => rsx! { FlaskConical { size } },
        StudioIconName::StatusRunning => rsx! { Play { size } },
        StudioIconName::StatusIdle => rsx! { CircleMinus { size } },
        StudioIconName::StatusError => rsx! { CircleAlert { size } },
    }
}

pub fn action_icon_name(icon: Option<&str>) -> Option<StudioIconName> {
    match icon {
        Some("play") => Some(StudioIconName::Play),
        Some("usb") => Some(StudioIconName::Usb),
        Some("test-tube") => Some(StudioIconName::Test),
        _ => None,
    }
}
