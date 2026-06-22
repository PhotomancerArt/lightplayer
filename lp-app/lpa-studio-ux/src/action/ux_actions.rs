use crate::{UxAction, UxNodeId};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UxActions {
    actions: Vec<UxAction>,
}

impl UxActions {
    pub fn new(actions: Vec<UxAction>) -> Self {
        Self { actions }
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &UxAction> {
        self.actions.iter()
    }

    pub fn push(&mut self, action: UxAction) {
        self.actions.push(action);
    }

    pub fn extend(&mut self, actions: impl IntoIterator<Item = UxAction>) {
        self.actions.extend(actions);
    }

    pub fn for_node(&self, node_id: &UxNodeId) -> Vec<UxAction> {
        self.actions
            .iter()
            .filter(|action| action.node_id() == node_id)
            .cloned()
            .collect()
    }

    pub fn for_node_id(&self, node_id: &str) -> Vec<UxAction> {
        self.actions
            .iter()
            .filter(|action| action.is_for_node(node_id))
            .cloned()
            .collect()
    }

    pub fn into_vec(self) -> Vec<UxAction> {
        self.actions
    }
}

impl From<Vec<UxAction>> for UxActions {
    fn from(actions: Vec<UxAction>) -> Self {
        Self::new(actions)
    }
}

impl IntoIterator for UxActions {
    type Item = UxAction;
    type IntoIter = std::vec::IntoIter<UxAction>;

    fn into_iter(self) -> Self::IntoIter {
        self.actions.into_iter()
    }
}
