/// Persistent buffer for one catalog text control. It is keyed by the exact
/// target selection so changing objects cannot carry uncommitted text across.
pub struct PropertyTextEditState {
    pub property: crate::properties::PropertyId,
    pub targets: Vec<crate::automation::TargetRef>,
    pub text: String,
    pub editing: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum XpsWorkbenchTab {
    #[default]
    Acquisition,
    Background,
    Components,
    Diagnostics,
}
