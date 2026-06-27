use dioxus::prelude::*;
use dioxus_icons::lucide::{
    Asterisk, Boxes, Check, ChevronDown, ChevronRight, CircleAlert, CircleDot, CircleMinus,
    FlaskConical, Info, Link2, Link2Off, Pencil, Play, SquareArrowRight, TriangleAlert, Usb,
};

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
        StudioIconName::StepComplete => rsx! { Check { size } },
        StudioIconName::StepActive => rsx! { Asterisk { size } },
        StudioIconName::StepAttention => rsx! { TriangleAlert { size } },
        StudioIconName::AssignedValue => rsx! { CircleDot { size } },
        StudioIconName::BoundValue => rsx! { Link2 { size } },
        StudioIconName::ChildValue => rsx! { SquareArrowRight { size } },
        StudioIconName::NodeTreeItem => rsx! { Boxes { size } },
        StudioIconName::Edited => rsx! { Pencil { size } },
        StudioIconName::Info => rsx! { Info { size } },
        StudioIconName::InfoBare => rsx! {
            span {
                class: "tw:inline-flex tw:items-center tw:justify-center tw:font-mono tw:font-bold",
                style: "font-size: {size}px; line-height: {size}px;",
                "i"
            }
        },
        StudioIconName::UnboundValue => rsx! { Link2Off { size } },
        StudioIconName::Expanded => rsx! { ChevronDown { size } },
        StudioIconName::Collapsed => rsx! { ChevronRight { size } },
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StudioIconName {
    Play,
    Usb,
    Test,
    StatusRunning,
    StatusIdle,
    StatusError,
    StepComplete,
    StepActive,
    StepAttention,
    AssignedValue,
    BoundValue,
    ChildValue,
    NodeTreeItem,
    Edited,
    Info,
    InfoBare,
    UnboundValue,
    Expanded,
    Collapsed,
}
