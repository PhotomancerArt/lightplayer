use crate::{UiAction, UxNodeId};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiActions {
    actions: Vec<UiAction>,
}

impl UiActions {
    pub fn new(actions: Vec<UiAction>) -> Self {
        Self { actions }
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &UiAction> {
        self.actions.iter()
    }

    pub fn push(&mut self, action: UiAction) {
        self.actions.push(action);
    }

    pub fn extend(&mut self, actions: impl IntoIterator<Item = UiAction>) {
        self.actions.extend(actions);
    }

    pub fn for_node(&self, node_id: &UxNodeId) -> Vec<UiAction> {
        self.actions
            .iter()
            .filter(|action| action.node_id() == node_id)
            .cloned()
            .collect()
    }

    pub fn for_node_id(&self, node_id: &str) -> Vec<UiAction> {
        self.actions
            .iter()
            .filter(|action| action.is_for_node(node_id))
            .cloned()
            .collect()
    }

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
