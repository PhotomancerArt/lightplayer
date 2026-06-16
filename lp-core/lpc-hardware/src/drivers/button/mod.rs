//! Button input drivers and debounced button events.
//!
//! Button endpoints claim GPIO input resources and return a
//! [`ButtonInput`](crate::ButtonInput). The common
//! [`ButtonDebouncer`](crate::ButtonDebouncer) keeps firmware and virtual
//! drivers aligned on when raw level changes become stable
//! [`ButtonEvent`](crate::ButtonEvent)s.

pub mod button_debouncer;
pub mod button_driver;
pub mod button_event;
pub mod virtual_button;
pub mod virtual_button_driver;
