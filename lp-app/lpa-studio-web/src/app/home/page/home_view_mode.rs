//! How the home page draws its boards and projects: as cards or as rows.
//!
//! Pure view state, not a user verb (PD4): the switch only changes how the
//! page draws what core listed, so it is no `UiAction` and not in the offer
//! tree. The browser remembers the choice in `localStorage` under one key,
//! [`HOME_VIEW_KEY`], in the manner of the visitor banner's dismissal
//! (`app/share/visitor_banner_dismissal.rs`): every access tolerates a
//! blocked or throwing store (private mode, Safari quirks) by reading as
//! cards and writing nothing, and any value but `"list"` reads as cards, so
//! there is no format to migrate.

/// The `localStorage` key for the cards/list switch.
pub(crate) const HOME_VIEW_KEY: &str = "lp.home.view.v1";

/// Cards (the default) or rows.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HomeViewMode {
    #[default]
    Cards,
    List,
}

impl HomeViewMode {
    /// The value stored: `"cards"` or `"list"`.
    pub(crate) const fn stored(self) -> &'static str {
        match self {
            Self::Cards => "cards",
            Self::List => "list",
        }
    }

    /// The mode a stored value names; anything but `"list"` is cards.
    pub(crate) fn from_stored(value: &str) -> Self {
        match value {
            "list" => Self::List,
            _ => Self::Cards,
        }
    }

    /// The remembered mode. Any storage failure, or nothing remembered,
    /// reads as cards.
    pub(crate) fn load() -> Self {
        read_from(slot().as_ref())
    }

    /// Remember the mode. A failed write is harmless: the page keeps the
    /// mode in its own signal for this load and asks again next time.
    pub(crate) fn save(self) {
        write_to(slot().as_ref(), self);
    }
}

/// Where the mode is remembered: one value in one place. A trait so the
/// reading and writing rules are testable without a browser.
trait ModeSlot {
    fn get(&self) -> Option<String>;
    fn set(&self, value: &str);
}

fn read_from(slot: Option<&impl ModeSlot>) -> HomeViewMode {
    slot.and_then(ModeSlot::get)
        .map(|value| HomeViewMode::from_stored(&value))
        .unwrap_or_default()
}

fn write_to(slot: Option<&impl ModeSlot>, mode: HomeViewMode) {
    if let Some(slot) = slot {
        slot.set(mode.stored());
    }
}

/// The browser's `localStorage` entry for [`HOME_VIEW_KEY`].
impl ModeSlot for web_sys::Storage {
    fn get(&self) -> Option<String> {
        self.get_item(HOME_VIEW_KEY).ok().flatten()
    }

    fn set(&self, value: &str) {
        let _ = self.set_item(HOME_VIEW_KEY, value);
    }
}

#[cfg(target_arch = "wasm32")]
fn slot() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// Host builds (tests, tooling) have no browser storage.
#[cfg(not(target_arch = "wasm32"))]
fn slot() -> Option<web_sys::Storage> {
    None
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn the_stored_values_round_trip() {
        for mode in [HomeViewMode::Cards, HomeViewMode::List] {
            assert_eq!(HomeViewMode::from_stored(mode.stored()), mode);
        }
        assert_eq!(HomeViewMode::Cards.stored(), "cards");
        assert_eq!(HomeViewMode::List.stored(), "list");
    }

    #[test]
    fn anything_but_list_reads_as_cards() {
        for value in ["cards", "", "List", "grid", "1", "list ", "null"] {
            assert_eq!(HomeViewMode::from_stored(value), HomeViewMode::Cards);
        }
        assert_eq!(HomeViewMode::default(), HomeViewMode::Cards);
    }

    #[test]
    fn the_key_is_the_one_the_plan_names() {
        assert_eq!(HOME_VIEW_KEY, "lp.home.view.v1");
    }

    /// With no storage (a host build, a blocked store) the page reads cards
    /// and writes nothing.
    #[test]
    fn without_storage_it_reads_cards_and_writes_nothing() {
        assert_eq!(HomeViewMode::load(), HomeViewMode::Cards);
        HomeViewMode::List.save();
        assert_eq!(
            HomeViewMode::load(),
            HomeViewMode::Cards,
            "nothing was remembered"
        );
        assert_eq!(read_from(None::<&Fake>), HomeViewMode::Cards);
        write_to(None::<&Fake>, HomeViewMode::List);
    }

    #[test]
    fn a_remembered_mode_is_read_back_and_a_stranger_reads_as_cards() {
        let slot = Fake::default();
        assert_eq!(read_from(Some(&slot)), HomeViewMode::Cards, "nothing yet");
        write_to(Some(&slot), HomeViewMode::List);
        assert_eq!(slot.0.borrow().as_deref(), Some("list"));
        assert_eq!(read_from(Some(&slot)), HomeViewMode::List);
        write_to(Some(&slot), HomeViewMode::Cards);
        assert_eq!(slot.0.borrow().as_deref(), Some("cards"));
        assert_eq!(read_from(Some(&slot)), HomeViewMode::Cards);
        *slot.0.borrow_mut() = Some("{\"view\":\"list\"}".to_string());
        assert_eq!(read_from(Some(&slot)), HomeViewMode::Cards);
    }

    #[derive(Default)]
    struct Fake(RefCell<Option<String>>);

    impl ModeSlot for Fake {
        fn get(&self) -> Option<String> {
            self.0.borrow().clone()
        }

        fn set(&self, value: &str) {
            *self.0.borrow_mut() = Some(value.to_string());
        }
    }
}
