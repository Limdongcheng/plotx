//! Lifetime of the one-shot Reference on-plot pick: it is valid exactly while
//! the step editor that armed it stays expanded on the active dataset.

use super::*;
use crate::state::{PhaseAxis, ReferencePick};
use plotx_processing::{ProcessingStep, ReferenceParams, StepId, StepKind, StepSource};

fn add_reference_step(app: &mut PlotxApp) -> StepId {
    let pipe = app.doc.datasets[0]
        .axis_pipeline_mut(PhaseAxis::Direct)
        .unwrap();
    let id = StepId::new(pipe.steps.iter().map(|s| s.id.get()).max().unwrap_or(0) + 1);
    pipe.steps.push(ProcessingStep::new(
        id,
        StepKind::Reference(ReferenceParams {
            at_ppm: 0.0,
            target_ppm: 0.0,
        }),
        StepSource::User,
    ));
    id
}

#[test]
fn reference_pick_resolves_only_while_its_editor_is_expanded() {
    let mut app = sample_app();
    let step = add_reference_step(&mut app);
    let dataset = dataset_id(&app, 0);

    app.toggle_reference_pick(dataset, step);
    assert_eq!(
        app.session.ui.reference_pick,
        Some(ReferencePick { dataset, step })
    );
    // Armed but the editor is not expanded: not resolvable, and sync drops it.
    assert!(app.resolve_reference_pick().is_none());
    app.sync_reference_pick();
    assert!(app.session.ui.reference_pick.is_none());

    app.session.ui.proc_expanded_step = Some((dataset, step));
    app.toggle_reference_pick(dataset, step);
    let resolved = app
        .resolve_reference_pick()
        .expect("expanded editor arms a valid pick");
    assert_eq!(resolved.dataset_index, 0);
    assert_eq!(resolved.axis, PhaseAxis::Direct);
    assert_eq!(resolved.pick, ReferencePick { dataset, step });

    // Collapsing the editor invalidates the pick; sync clears the arm state.
    app.session.ui.proc_expanded_step = None;
    assert!(app.resolve_reference_pick().is_none());
    app.sync_reference_pick();
    assert!(app.session.ui.reference_pick.is_none());
}

#[test]
fn reference_pick_toggle_disarms_and_a_non_reference_step_never_resolves() {
    let mut app = sample_app();
    let step = add_reference_step(&mut app);
    let dataset = dataset_id(&app, 0);
    app.session.ui.proc_expanded_step = Some((dataset, step));

    app.toggle_reference_pick(dataset, step);
    assert!(app.resolve_reference_pick().is_some());
    app.toggle_reference_pick(dataset, step);
    assert!(app.session.ui.reference_pick.is_none());

    // A pick armed for a step that is not a Reference step must not resolve —
    // the pick would otherwise write at_ppm into a foreign step.
    let phase_step = app.doc.datasets[0]
        .axis_pipeline(PhaseAxis::Direct)
        .unwrap()
        .steps
        .iter()
        .find(|s| matches!(s.kind, StepKind::Phase(_)))
        .unwrap()
        .id;
    app.session.ui.proc_expanded_step = Some((dataset, phase_step));
    app.toggle_reference_pick(dataset, phase_step);
    assert!(app.resolve_reference_pick().is_none());
    app.sync_reference_pick();
    assert!(app.session.ui.reference_pick.is_none());
}
