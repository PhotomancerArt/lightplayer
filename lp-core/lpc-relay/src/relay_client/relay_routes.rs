//! The board's open routes.

use alloc::vec::Vec;

/// The routes a board holds open, at most `max` of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayRoutes {
    max: usize,
    open: Vec<u16>,
}

impl RelayRoutes {
    /// A table for a board that holds `max` sessions.
    #[must_use]
    pub fn new(max: usize) -> Self {
        Self {
            max,
            open: Vec::new(),
        }
    }

    /// Open `route`: `false` when the table is full (the board answers
    /// `Busy`). Opening a route already open is a yes, and changes nothing.
    pub fn open(&mut self, route: u16) -> bool {
        if self.contains(route) {
            return true;
        }
        if self.open.len() >= self.max {
            return false;
        }
        self.open.push(route);
        true
    }

    /// Close `route`: whether it was open.
    pub fn close(&mut self, route: u16) -> bool {
        let before = self.open.len();
        self.open.retain(|open| *open != route);
        self.open.len() != before
    }

    #[must_use]
    pub fn contains(&self, route: u16) -> bool {
        self.open.contains(&route)
    }

    /// Every open route, removed: the device leg closed.
    pub fn take_all(&mut self) -> Vec<u16> {
        core::mem::take(&mut self.open)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.open.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_of_one_refuses_a_second_route() {
        let mut routes = RelayRoutes::new(1);
        assert!(routes.open(4));
        assert!(routes.open(4), "the same route again is not a second one");
        assert!(!routes.open(5));
        assert!(routes.close(4));
        assert!(!routes.close(4));
        assert!(routes.open(5));
        assert_eq!(routes.take_all(), [5]);
        assert!(routes.is_empty());
    }
}
