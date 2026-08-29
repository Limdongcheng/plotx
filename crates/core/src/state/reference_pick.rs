//! The one-shot "pick a Reference position on the plot" arm state.

use super::{DatasetId, PhaseAxis, StepId};

/// A one-shot request to pick a Reference step's source position (`at_ppm`) on
/// the plot. Typed IDs, because the arm state outlives the frame that set it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferencePick {
    pub dataset: DatasetId,
    pub step: StepId,
}

/// An armed [`ReferencePick`] resolved against the current document: the pick
/// plus the one-shot dataset index and the axis whose pipeline owns the step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedReferencePick {
    pub pick: ReferencePick,
    pub dataset_index: usize,
    pub axis: PhaseAxis,
}
