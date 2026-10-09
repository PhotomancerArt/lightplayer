//! "On <boards>": which boards play a project, in the words the project
//! card, its ⋯ menu and its list row share.
//!
//! The names are the join's answer (`UiPackageCard::on_boards`, stamped by
//! core); nothing is joined here. Two names fit on a line; more read as a
//! count.

/// The line for a project played by `boards`; `None` when no board plays
/// it.
pub(crate) fn boards_line(boards: &[String]) -> Option<String> {
    match boards {
        [] => None,
        [one] => Some(format!("On {one}")),
        [first, second] => Some(format!("On {first}, {second}")),
        more => Some(format!("On {} boards", more.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn no_board_says_nothing() {
        assert_eq!(boards_line(&[]), None);
    }

    #[test]
    fn one_board_is_named() {
        assert_eq!(
            boards_line(&names(&["Desk C6"])).as_deref(),
            Some("On Desk C6")
        );
    }

    #[test]
    fn two_boards_are_both_named() {
        assert_eq!(
            boards_line(&names(&["Desk C6", "Porch"])).as_deref(),
            Some("On Desk C6, Porch")
        );
    }

    #[test]
    fn three_boards_read_as_a_count() {
        assert_eq!(
            boards_line(&names(&["Desk C6", "Porch", "Truck"])).as_deref(),
            Some("On 3 boards")
        );
    }
}
