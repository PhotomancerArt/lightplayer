/// State holder that exposes the actions currently available to users or agents.
pub trait UxManager {
    type State;
    type Action;

    fn state(&self) -> &Self::State;
    fn available_actions(&self) -> Vec<Self::Action>;
}
