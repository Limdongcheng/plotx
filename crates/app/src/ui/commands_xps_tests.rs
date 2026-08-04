use super::*;
use plotx_core::actions::Action;
use plotx_core::state::{DEFAULT_CANVAS_SIZE_MM, XpsDataset};
use plotx_io::xps::{
    XpsEnergyKind, XpsExperiment, XpsMeasurement, XpsMeasurementId, XpsRegion, XpsRegionId,
};

fn app_with_xps() -> PlotxApp {
    let mut app = PlotxApp::new_with_settings(plotx_core::settings::Settings::default());
    let measurement = XpsMeasurementId(1);
    let experiment = XpsExperiment {
        source: "commands.vms".to_owned(),
        measurements: vec![XpsMeasurement {
            id: measurement,
            label: "Location 1".to_owned(),
            position_mm: None,
            metadata: Default::default(),
        }],
        regions: vec![XpsRegion {
            id: XpsRegionId(1),
            measurement,
            name: "C 1s".to_owned(),
            native_energy_kind: XpsEnergyKind::Binding,
            native_energy_ev: vec![286.0, 285.0, 284.0],
            binding_energy_ev: Some(vec![286.0, 285.0, 284.0]),
            intensity_cps: vec![1.0, 3.0, 1.0],
            counts: None,
            photon_energy_ev: Some(1486.69),
            dwell_time_s: None,
            sweeps: None,
            imported_fit: None,
            metadata: Default::default(),
        }],
        metadata: Default::default(),
        import_warnings: Vec::new(),
    };
    let action = Action::insert_dataset_with_default_canvas(
        &app,
        Dataset::Xps(Box::new(XpsDataset::load(experiment))),
        "Canvas — XPS".to_owned(),
        DEFAULT_CANVAS_SIZE_MM,
    );
    app.execute_action(action);
    app
}

#[test]
fn xps_uses_its_workbench_instead_of_generic_peak_commands() {
    let app = app_with_xps();

    for command in [
        CommandId::PeakList,
        CommandId::LineFit,
        CommandId::RunPeakFit,
        CommandId::Integrate,
        CommandId::Multiplets,
    ] {
        let descriptor = describe(&app, command);
        assert!(!descriptor.enabled);
        assert_eq!(descriptor.ribbon, None);
    }

    assert!(describe(&app, CommandId::SelectRange).enabled);
}
