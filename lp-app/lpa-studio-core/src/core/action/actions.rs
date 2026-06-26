use crate::{ControllerId, UiAction};

/// A collection of user-invokable actions.
///
/// Use this when actions need to be gathered, filtered by target controller,
/// and passed through view construction before they are rendered.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiActions {
    actions: Vec<UiAction>,
}

impl UiActions {
    /// Create an action collection from a vector.
    pub fn new(actions: Vec<UiAction>) -> Self {
        Self { actions }
    }

    /// Return whether the collection has no actions.
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Return the number of actions in the collection.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Iterate over actions in display order.
    pub fn iter(&self) -> impl Iterator<Item = &UiAction> {
        self.actions.iter()
    }

    /// Append an action.
    pub fn push(&mut self, action: UiAction) {
        self.actions.push(action);
    }

    /// Append multiple actions.
    pub fn extend(&mut self, actions: impl IntoIterator<Item = UiAction>) {
        self.actions.extend(actions);
    }

    /// Return actions targeting a controller id.
    pub fn for_node(&self, node_id: &ControllerId) -> Vec<UiAction> {
        self.actions
            .iter()
            .filter(|action| action.node_id() == node_id)
            .cloned()
            .collect()
    }

    /// Return actions targeting a controller id string.
    pub fn for_node_id(&self, node_id: &str) -> Vec<UiAction> {
        self.actions
            .iter()
            .filter(|action| action.is_for_node(node_id))
            .cloned()
            .collect()
    }

    /// Consume the collection and return the underlying vector.
    pub fn into_vec(self) -> Vec<UiAction> {
        self.actions
    }
}

impl From<Vec<UiAction>> for UiActions {
    fn from(actions: Vec<UiAction>) -> Self {
        Self::new(actions)
    }
}

impl IntoIterator for UiActions {
    type Item = UiAction;
    type IntoIter = std::vec::IntoIter<UiAction>;

    fn into_iter(self) -> Self::IntoIter {
        self.actions.into_iter()
    }
}
