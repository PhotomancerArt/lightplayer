//! Drawing geometry section: board width, module/can, USB connectors,
//! buttons, and the onboard RGB pixel. All values are in drawing units —
//! authors watch the live preview, not the numbers.

use dioxus::prelude::*;
use lpa_boards::{DrawnButton, DrawnRgb, DrawnUsb};

use crate::editor_core::editor_doc::EditorDoc;
use crate::view::form_widgets::{CheckField, NumField, OptGpioField, TextField};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DrawingSection(doc: Signal<EditorDoc>) -> Element {
    let hw = doc.read().board.hw.clone();

    rsx! {
        section { class: "lpb-ed-section",
            h2 { "Drawing" }
            div { class: "lpb-ed-grid",
                NumField {
                    label: "board width (units)",
                    value: f64::from(hw.width),
                    on_change: move |value: f64| doc.write().edit(|b| b.hw.width = value as f32),
                }
            }
            h3 { "Module" }
            div { class: "lpb-ed-grid",
                NumField {
                    label: "x",
                    value: f64::from(hw.module.x),
                    on_change: move |value: f64| doc.write().edit(|b| b.hw.module.x = value as f32),
                }
                NumField {
                    label: "y",
                    value: f64::from(hw.module.y),
                    on_change: move |value: f64| doc.write().edit(|b| b.hw.module.y = value as f32),
                }
                NumField {
                    label: "w",
                    value: f64::from(hw.module.w),
                    on_change: move |value: f64| doc.write().edit(|b| b.hw.module.w = value as f32),
                }
                NumField {
                    label: "h",
                    value: f64::from(hw.module.h),
                    on_change: move |value: f64| doc.write().edit(|b| b.hw.module.h = value as f32),
                }
                TextField {
                    label: "label",
                    value: hw.module.label.clone(),
                    mono: true,
                    on_change: move |value| doc.write().edit(|b| b.hw.module.label = value),
                }
            }
            CheckField {
                label: "antenna keep-out strip",
                value: hw.module.antenna,
                on_change: move |value| doc.write().edit(|b| b.hw.module.antenna = value),
            }
            h3 { "USB connectors" }
            div { class: "lpb-ed-rows",
                for (index, usb) in hw.usb.iter().enumerate() {
                    div { key: "{index}", class: "lpb-ed-row",
                        NumField {
                            label: "x",
                            value: f64::from(usb.x),
                            on_change: move |value: f64| {
                                doc.write().edit(|b| {
                                    if let Some(entry) = b.hw.usb.get_mut(index) {
                                        entry.x = value as f32;
                                    }
                                });
                            },
                        }
                        input {
                            r#type: "text",
                            class: "lpb-ed-input lpb-ed-row-label",
                            placeholder: "label",
                            value: "{usb.label}",
                            oninput: move |event| {
                                doc.write().edit(|b| {
                                    if let Some(entry) = b.hw.usb.get_mut(index) {
                                        entry.label = event.value();
                                    }
                                });
                            },
                        }
                        button {
                            class: "lpb-ed-chip-x",
                            title: "remove connector",
                            onclick: move |_| {
                                doc.write().edit(|b| {
                                    if index < b.hw.usb.len() {
                                        b.hw.usb.remove(index);
                                    }
                                });
                            },
                            "×"
                        }
                    }
                }
                button {
                    class: "lpb-ed-add-row",
                    onclick: move |_| {
                        doc.write().edit(|b| {
                            b.hw.usb.push(DrawnUsb {
                                x: b.hw.width / 2.0,
                                label: "USB".into(),
                            });
                        });
                    },
                    "+ usb"
                }
            }
            h3 { "Buttons" }
            div { class: "lpb-ed-rows",
                for (index, button_def) in hw.buttons.iter().enumerate() {
                    div { key: "{index}", class: "lpb-ed-row",
                        NumField {
                            label: "x",
                            value: f64::from(button_def.x),
                            on_change: move |value: f64| {
                                doc.write().edit(|b| {
                                    if let Some(entry) = b.hw.buttons.get_mut(index) {
                                        entry.x = value as f32;
                                    }
                                });
                            },
                        }
                        NumField {
                            label: "y (neg = from bottom)",
                            value: f64::from(button_def.y),
                            on_change: move |value: f64| {
                                doc.write().edit(|b| {
                                    if let Some(entry) = b.hw.buttons.get_mut(index) {
                                        entry.y = value as f32;
                                    }
                                });
                            },
                        }
                        input {
                            r#type: "text",
                            class: "lpb-ed-input lpb-ed-row-label",
                            placeholder: "label",
                            value: "{button_def.label}",
                            oninput: move |event| {
                                doc.write().edit(|b| {
                                    if let Some(entry) = b.hw.buttons.get_mut(index) {
                                        entry.label = event.value();
                                    }
                                });
                            },
                        }
                        button {
                            class: "lpb-ed-chip-x",
                            title: "remove button",
                            onclick: move |_| {
                                doc.write().edit(|b| {
                                    if index < b.hw.buttons.len() {
                                        b.hw.buttons.remove(index);
                                    }
                                });
                            },
                            "×"
                        }
                    }
                }
                button {
                    class: "lpb-ed-add-row",
                    onclick: move |_| {
                        doc.write().edit(|b| {
                            b.hw.buttons.push(DrawnButton {
                                x: 10.0,
                                y: -14.0,
                                label: "BOOT".into(),
                            });
                        });
                    },
                    "+ button"
                }
            }
            h3 { "Onboard RGB pixel" }
            if let Some(rgb) = hw.rgb {
                div { class: "lpb-ed-row",
                    NumField {
                        label: "x",
                        value: f64::from(rgb.x),
                        on_change: move |value: f64| {
                            doc.write().edit(|b| {
                                if let Some(entry) = b.hw.rgb.as_mut() {
                                    entry.x = value as f32;
                                }
                            });
                        },
                    }
                    NumField {
                        label: "y",
                        value: f64::from(rgb.y),
                        on_change: move |value: f64| {
                            doc.write().edit(|b| {
                                if let Some(entry) = b.hw.rgb.as_mut() {
                                    entry.y = value as f32;
                                }
                            });
                        },
                    }
                    OptGpioField {
                        label: "gpio",
                        value: rgb.gpio,
                        on_change: move |value| {
                            doc.write().edit(|b| {
                                if let Some(entry) = b.hw.rgb.as_mut() {
                                    entry.gpio = value;
                                }
                            });
                        },
                    }
                    button {
                        class: "lpb-ed-chip-x",
                        title: "remove rgb pixel",
                        onclick: move |_| doc.write().edit(|b| b.hw.rgb = None),
                        "×"
                    }
                }
            } else {
                button {
                    class: "lpb-ed-add-row",
                    onclick: move |_| {
                        doc.write().edit(|b| {
                            b.hw.rgb = Some(DrawnRgb {
                                x: b.hw.width / 2.0,
                                y: b.hw.module.y + b.hw.module.h + 8.0,
                                gpio: None,
                            });
                        });
                    },
                    "+ rgb pixel"
                }
            }
        }
    }
}
