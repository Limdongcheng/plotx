use super::*;
use crate::actions::DatasetProcessingState;
use crate::state::XrdDataset;
use plotx_processing::xrd::{MAX_SNIP_ITERATIONS, SavitzkyGolay, SnipBackground, XrdProcessing};

fn xrd_app() -> PlotxApp {
    xrd_app_with_intensity(vec![10.0, 25.0, 12.0])
}

fn xrd_app_with_intensity(intensity: Vec<f64>) -> PlotxApp {
    let mut app = PlotxApp::new();
    let two_theta_deg = (0..intensity.len())
        .map(|index| 3.0 + index as f64 * 0.1)
        .collect();
    let data = plotx_io::XrdData {
        two_theta_deg,
        intensity,
        attenuation: None,
        source: "test.rasx".to_owned(),
        instrument: None,
        target: None,
        wavelength_angstrom: None,
        voltage_kv: None,
        current_ma: None,
        scan_step_deg: Some(0.1),
        scan_speed_deg_min: None,
    };
    app.doc
        .datasets
        .push(Dataset::Xrd(Box::new(XrdDataset::load(data))));
    app
}

#[test]
fn invalid_xrd_processing_does_not_mutate_or_enter_history() {
    let mut app = xrd_app();
    let dataset_id = app.doc.datasets[0].resource_id();
    let before = DatasetProcessingState::from_dataset(&app.doc.datasets[0]);
    let before_processed = app.doc.datasets[0].as_xrd().unwrap().processed.clone();
    let invalid = DatasetProcessingState::Xrd(XrdProcessing {
        background: Some(SnipBackground {
            iterations: MAX_SNIP_ITERATIONS + 1,
        }),
        ..XrdProcessing::default()
    });

    let error = app
        .try_execute_action(Action::update_dataset_processing(
            dataset_id,
            before.clone(),
            invalid,
        ))
        .unwrap_err();

    assert!(error.to_string().contains("invalid XRD processing"));
    assert_eq!(
        DatasetProcessingState::from_dataset(&app.doc.datasets[0]),
        before
    );
    assert_eq!(
        app.doc.datasets[0].as_xrd().unwrap().processed,
        before_processed
    );
    assert!(app.session.undo_stack.is_empty());
    assert!(!app.doc.dirty);
}

#[test]
fn invalid_xrd_processing_state_apply_is_transactional() {
    let mut app = xrd_app();
    let before = DatasetProcessingState::from_dataset(&app.doc.datasets[0]);
    let before_processed = app.doc.datasets[0].as_xrd().unwrap().processed.clone();
    let invalid = DatasetProcessingState::Xrd(XrdProcessing {
        background: Some(SnipBackground {
            iterations: MAX_SNIP_ITERATIONS + 1,
        }),
        ..XrdProcessing::default()
    });

    invalid.apply_to(&mut app.doc.datasets[0]).unwrap_err();

    assert_eq!(
        DatasetProcessingState::from_dataset(&app.doc.datasets[0]),
        before
    );
    assert_eq!(
        app.doc.datasets[0].as_xrd().unwrap().processed,
        before_processed
    );
}

#[test]
fn numerical_xrd_processing_failure_does_not_enter_history() {
    let mut app = xrd_app_with_intensity(vec![0.0, f64::MAX, f64::MAX, f64::MAX, 0.0]);
    let dataset_id = app.doc.datasets[0].resource_id();
    let before = DatasetProcessingState::from_dataset(&app.doc.datasets[0]);
    let before_processed = app.doc.datasets[0].as_xrd().unwrap().processed.clone();
    let after = DatasetProcessingState::Xrd(XrdProcessing {
        smoothing: Some(SavitzkyGolay {
            window: 5,
            polynomial_order: 2,
        }),
        ..XrdProcessing::default()
    });

    let error = app
        .try_execute_action(Action::update_dataset_processing(
            dataset_id,
            before.clone(),
            after,
        ))
        .unwrap_err();

    assert!(error.to_string().contains("non-finite"));
    assert_eq!(
        DatasetProcessingState::from_dataset(&app.doc.datasets[0]),
        before
    );
    assert_eq!(
        app.doc.datasets[0].as_xrd().unwrap().processed,
        before_processed
    );
    assert!(app.session.undo_stack.is_empty());
    assert!(!app.doc.dirty);
}

#[test]
fn composite_rolls_back_when_xrd_processing_fails() {
    let mut app = xrd_app_with_intensity(vec![0.0, f64::MAX, f64::MAX, f64::MAX, 0.0]);
    app.doc
        .canvases
        .push(CanvasDocument::new("Before".to_owned(), [100.0, 80.0]));
    let dataset_id = app.doc.datasets[0].resource_id();
    let before = DatasetProcessingState::from_dataset(&app.doc.datasets[0]);
    let failing = DatasetProcessingState::Xrd(XrdProcessing {
        smoothing: Some(SavitzkyGolay {
            window: 5,
            polynomial_order: 2,
        }),
        ..XrdProcessing::default()
    });
    let action = Action::Composite(vec![
        Action::rename_canvas(0, "Before".to_owned(), "After".to_owned()),
        Action::update_dataset_processing(dataset_id, before, failing),
    ]);

    app.try_execute_action(action).unwrap_err();

    assert_eq!(app.doc.canvases[0].name, "Before");
    assert!(app.session.undo_stack.is_empty());
    assert!(!app.doc.dirty);
}

#[test]
fn paused_xrd_processing_uses_the_shared_apply_path() {
    let mut app = xrd_app();
    let before = DatasetProcessingState::from_dataset(&app.doc.datasets[0]);
    let before_processed = app.doc.datasets[0].as_xrd().unwrap().processed.clone();
    let after = DatasetProcessingState::Xrd(XrdProcessing {
        background: Some(SnipBackground { iterations: 1 }),
        ..XrdProcessing::default()
    });
    app.session.ui.proc_paused = true;

    app.commit_processing_edit(0, before.clone(), after.clone());

    assert_eq!(
        DatasetProcessingState::from_dataset(&app.doc.datasets[0]),
        after
    );
    assert!(app.has_pending_processing());
    assert!(app.session.undo_stack.is_empty());

    app.apply_paused_processing();

    assert_eq!(
        DatasetProcessingState::from_dataset(&app.doc.datasets[0]),
        after
    );
    assert!(!app.has_pending_processing());
    assert_eq!(app.session.undo_stack.len(), 1);
    assert_ne!(
        app.doc.datasets[0].as_xrd().unwrap().processed,
        before_processed
    );
}
