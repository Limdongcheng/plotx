use super::*;
use crate::state::{StoredXpsFit, XpsDataset, xps_input_sha256};
use plotx_analysis::xps::{
    XpsBootstrapPeak, XpsBootstrapResult, XpsComponentId, XpsPeakSpec, fit_xps_peaks,
};
use plotx_io::xps::{
    ImportedXpsFit, ImportedXpsPeak, XpsEnergyKind, XpsExperiment, XpsMeasurement,
    XpsMeasurementId, XpsRegion, XpsRegionId,
};
use plotx_processing::xps::{XpsProcessingStep, XpsStepKind};
use plotx_processing::{NormalizeMethod, StepId, StepSource};
use std::collections::BTreeMap;
use std::io::Read;

#[test]
fn xps_project_roundtrip_preserves_hierarchy_recipes_and_analyses_without_schema_bump() {
    let measurement = XpsMeasurementId(41);
    let region_id = XpsRegionId(73);
    let energy = vec![290.0, 289.0, 288.0, 287.0, 286.0, 285.0, 284.0, 283.0];
    let intensity = vec![3.0, 4.0, 6.0, 12.0, 20.0, 60.0, 18.0, 5.0];
    let imported = ImportedXpsFit {
        background_cps: vec![3.0; 8],
        envelope_cps: intensity.clone(),
        components_cps: vec![intensity.iter().map(|value| value - 3.0).collect()],
        peaks: vec![ImportedXpsPeak {
            label: "C 1s".into(),
            position_ev: 284.8,
            fwhm_ev: 1.2,
            area: 42.0,
            lineshape: Some("LA(50)".into()),
        }],
    };
    let experiment = XpsExperiment {
        source: "synthetic.vms".into(),
        measurements: vec![XpsMeasurement {
            id: measurement,
            label: "Location 2".into(),
            position_mm: Some([1.0, 2.0, 3.0]),
            metadata: BTreeMap::from([("sample".into(), "reference".into())]),
        }],
        regions: vec![XpsRegion {
            id: region_id,
            measurement,
            name: "C 1s".into(),
            native_energy_kind: XpsEnergyKind::Binding,
            native_energy_ev: energy.clone(),
            binding_energy_ev: Some(energy.clone()),
            intensity_cps: intensity.clone(),
            counts: Some(intensity.iter().map(|value| value * 3.0).collect()),
            photon_energy_ev: Some(1486.69),
            dwell_time_s: Some(1.0),
            sweeps: Some(3),
            imported_fit: Some(imported),
            metadata: BTreeMap::new(),
        }],
        metadata: BTreeMap::new(),
        import_warnings: Vec::new(),
    };
    let mut xps = XpsDataset::load(experiment);
    *xps.measurement_shifts.get_mut(&measurement).unwrap() = 0.2;
    let recipe = xps.region_recipes.get_mut(&region_id).unwrap();
    recipe.steps.push(XpsProcessingStep {
        id: StepId::new(9),
        kind: XpsStepKind::Normalize(NormalizeMethod::MaxPeak),
        enabled: true,
        source: StepSource::User,
    });
    xps.next_step_id = 10;
    let processed = xps.processed_region(region_id).unwrap();
    let workspace = xps.fit_workspaces.get_mut(&region_id).unwrap();
    workspace.invocation.background =
        plotx_analysis::xps::XpsBackgroundSpec::suggested(&processed.binding_energy_ev).unwrap();
    workspace.invocation.peaks = vec![XpsPeakSpec::independent(
        XpsComponentId::new(1),
        "Aromatic C",
        285.2,
        20.0,
    )];
    workspace.next_component_id = 2;
    let invocation = workspace.invocation.clone();
    let result = fit_xps_peaks(
        &processed.binding_energy_ev,
        &processed.intensity,
        &invocation,
        &|| false,
    )
    .unwrap();
    let expected_r_squared = result.r_squared;
    let fit = StoredXpsFit {
        region: region_id,
        input_sha256: xps_input_sha256(
            region_id,
            &processed.binding_energy_ev,
            &processed.intensity,
            &invocation,
        ),
        energy_shift_ev: xps.measurement_shifts[&measurement],
        processing_recipe: xps.region_recipes[&region_id].clone(),
        invocation,
        result,
        bootstrap: Some(XpsBootstrapResult {
            requested: 100,
            converged: 96,
            seed: 42,
            peaks: vec![XpsBootstrapPeak {
                id: XpsComponentId::new(1),
                center_ev: [284.9, 285.2, 285.4],
                fwhm_ev: [0.9, 1.2, 1.5],
                area: [18.0, 20.0, 22.0],
                fraction: [1.0, 1.0, 1.0],
            }],
        }),
    };
    xps.fits.insert(region_id, vec![fit]);
    let mut app = PlotxApp::new();
    app.doc.datasets.push(Dataset::Xps(Box::new(xps)));
    let mut edited = app.doc.datasets[0].as_xps().unwrap().fit_workspaces[&region_id].clone();
    edited.invocation.peaks[0].label = "Edited assignment".into();
    app.set_xps_fit_workspace(app.doc.datasets[0].resource_id(), region_id, edited)
        .unwrap();
    assert_eq!(
        app.doc.datasets[0].as_xps().unwrap().fit_workspaces[&region_id]
            .invocation
            .peaks[0]
            .label,
        "Edited assignment"
    );
    app.undo();
    assert_eq!(
        app.doc.datasets[0].as_xps().unwrap().fit_workspaces[&region_id]
            .invocation
            .peaks[0]
            .label,
        "Aromatic C"
    );
    app.redo();
    assert_eq!(
        app.doc.datasets[0].as_xps().unwrap().fit_workspaces[&region_id]
            .invocation
            .peaks[0]
            .label,
        "Edited assignment"
    );
    app.undo();
    let path = super::tests::temp_project("xps-roundtrip");
    let _ = std::fs::remove_file(&path);

    save_project(&app, &path, false).unwrap();
    let loaded = load_project(&path).unwrap();
    let xps = loaded.doc.datasets[0].as_xps().unwrap();
    assert_eq!(xps.experiment.measurements[0].id, measurement);
    assert_eq!(xps.active_region, region_id);
    assert_eq!(xps.energy_shift(measurement), Some(0.2));
    assert_eq!(xps.recipe(region_id).unwrap().steps[0].id, StepId::new(9));
    assert_eq!(xps.next_step_id, 10);
    assert_eq!(xps.fit_workspaces[&region_id].next_component_id, 2);
    assert_eq!(
        xps.active_region().imported_fit.as_ref().unwrap().peaks[0]
            .lineshape
            .as_deref(),
        Some("LA(50)")
    );
    assert_eq!(
        xps.current_fit(region_id).unwrap().result.r_squared,
        expected_r_squared
    );
    assert_eq!(
        xps.current_fit(region_id).unwrap().result.energy_ev.len(),
        processed.binding_energy_ev.len()
    );
    assert_eq!(
        xps.current_fit(region_id)
            .unwrap()
            .bootstrap
            .as_ref()
            .unwrap()
            .seed,
        42
    );

    let file = std::fs::File::open(&path).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let manifest: Manifest = read_json(&mut zip, "manifest.json").unwrap();
    assert_eq!(manifest.schema_version, 1);
    let recipe_path = manifest
        .objects
        .iter()
        .find(|entry| entry.role == "recipe")
        .unwrap()
        .path
        .clone();
    let recipe: RecipeObject = read_json(&mut zip, &recipe_path).unwrap();
    let fits = recipe.extensions["plotx.xps"]["fits"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .as_array()
        .unwrap();
    let result_fields = fits[0]["result"].as_object().unwrap();
    for curve_field in [
        "energy_ev",
        "intensity",
        "background",
        "envelope",
        "residual",
        "components",
    ] {
        assert!(!result_fields.contains_key(curve_field));
    }
    let payload = zip
        .file_names()
        .find(|name| name.ends_with("/data.bin"))
        .unwrap()
        .to_owned();
    let mut entry = zip.by_name(&payload).unwrap();
    let mut magic = [0_u8; 8];
    entry.read_exact(&mut magic).unwrap();
    assert_eq!(&magic, b"PLOTXXPS");
    std::fs::remove_file(path).unwrap();
}
