//! Catch a candidate's panic and score it, without spraying stderr.

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

thread_local! {
    static QUIET: Cell<bool> = const { Cell::new(false) };
    static LAST_LOCATION: RefCell<String> = const { RefCell::new(String::new()) };
}

static HOOK: Once = Once::new();

/// Run `f`; a panic comes back as `Err("<message> at <file:line>")`.
pub fn catch_quiet<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    HOOK.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if QUIET.with(Cell::get) {
                let loc = info
                    .location()
                    .map(|l| format!("{}:{}", l.file(), l.line()))
                    .unwrap_or_default();
                LAST_LOCATION.with(|l| *l.borrow_mut() = loc);
            } else {
                default(info);
            }
        }));
    });
    let was = QUIET.with(|q| q.replace(true));
    let r = catch_unwind(AssertUnwindSafe(f));
    QUIET.with(|q| q.set(was));
    r.map_err(|payload| {
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic".into());
        let loc = LAST_LOCATION.with(|l| l.borrow().clone());
        format!("{msg} at {loc}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catches_and_reports() {
        assert_eq!(catch_quiet(|| 3), Ok(3));
        let e = catch_quiet(|| panic!("boom")).unwrap_err();
        assert!(e.starts_with("boom at "), "{e}");
    }
}
