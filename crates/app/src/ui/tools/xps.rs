mod background;
mod components;
mod diagnostics;

use egui::{Button, ComboBox, DragValue, Ui};
use egui_phosphor::regular as icon;
use plotx_core::state::{
    DatasetId, PlotxApp, XpsFitWorkspace, XpsWorkbenchTab, estimate_xps_charge_shift,
};
use plotx_processing::xps::XpsStepKind;
use plotx_processing::{NormalizeMethod, SmoothMethod};

#[derive(Clone, Default)]
struct ShiftEdit {
    value: f64,
    changed: bool,
}

#[derive(Clone)]
struct WindowDraft {
    low_ev: f64,
    high_ev: f64,
}

pub(super) fn xps_group(app: &mut PlotxApp, dataset_index: usize, ui: &mut Ui) -> bool {
    let Some(xps) = app
        .doc
        .datasets
        .get(dataset_index)
        .and_then(|dataset| dataset.as_xps())
    else {
        return false;
    };
    let dataset_id = xps.resource_id;
    let active = xps.active_region().clone();
    let measurements = xps.experiment.measurements.clone();
    let regions = xps.experiment.regions.clone();
    let energy_shift_ev = xps.energy_shift(active.measurement).unwrap_or_default();
    let recipe = xps.recipe(active.id).cloned().unwrap_or_default();
    let processed = xps.displayed_region(active.id);
    let workspace = xps.fit_workspaces.get(&active.id).cloned();
    let current_fit = xps.current_fit(active.id).cloned();
    let latest_fit = xps.latest_fit(active.id).cloned();

    let mut tab = app.session.ui.xps_workbench_tab;
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut tab, XpsWorkbenchTab::Acquisition, "Acquisition");
        ui.selectable_value(&mut tab, XpsWorkbenchTab::Background, "Background");
        ui.selectable_value(&mut tab, XpsWorkbenchTab::Components, "Components");
        ui.selectable_value(&mut tab, XpsWorkbenchTab::Diagnostics, "Diagnostics");
    });
    app.session.ui.xps_workbench_tab = tab;
    ui.separator();

    match tab {
        XpsWorkbenchTab::Acquisition => acquisition_tab(
            app,
            dataset_index,
            dataset_id,
            &active,
            &measurements,
            &regions,
            energy_shift_ev,
            &recipe,
            processed.as_ref(),
            ui,
        ),
        XpsWorkbenchTab::Background => {
            if let Some(workspace) = workspace {
                background::background_tab(
                    app,
                    dataset_index,
                    dataset_id,
                    &active,
                    processed.as_ref(),
                    workspace,
                    ui,
                );
            } else {
                ui.weak("A binding-energy axis is required for background analysis.");
            }
        }
        XpsWorkbenchTab::Components => {
            if let Some(workspace) = workspace {
                components::components_tab(
                    app,
                    dataset_index,
                    dataset_id,
                    &active,
                    processed.as_ref(),
                    workspace,
                    ui,
                );
            } else {
                ui.weak("A binding-energy axis is required for peak fitting.");
            }
        }
        XpsWorkbenchTab::Diagnostics => diagnostics::diagnostics_tab(
            app,
            dataset_id,
            active.id,
            current_fit.as_ref(),
            latest_fit.as_ref(),
            ui,
        ),
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn acquisition_tab(
    app: &mut PlotxApp,
    dataset_index: usize,
    dataset_id: DatasetId,
    active: &plotx_io::xps::XpsRegion,
    measurements: &[plotx_io::xps::XpsMeasurement],
    regions: &[plotx_io::xps::XpsRegion],
    energy_shift_ev: f64,
    recipe: &plotx_processing::xps::XpsProcessingRecipe,
    processed: Option<&plotx_processing::xps::ProcessedXpsRegion>,
    ui: &mut Ui,
) {
    ui.label(crate::typography::headline("Acquisition"));
    let measurement_label = measurements
        .iter()
        .find(|measurement| measurement.id == active.measurement)
        .map_or("Unknown position", |measurement| measurement.label.as_str());
    let mut selected_measurement = None;
    ComboBox::from_label("Measurement position")
        .selected_text(measurement_label)
        .show_ui(ui, |ui| {
            for measurement in measurements {
                if ui
                    .selectable_label(measurement.id == active.measurement, &measurement.label)
                    .clicked()
                {
                    selected_measurement = Some(measurement.id);
                    ui.close();
                }
            }
        });
    let mut selected_region = None;
    ComboBox::from_label("Spectrum region")
        .selected_text(&active.name)
        .show_ui(ui, |ui| {
            for region in regions
                .iter()
                .filter(|region| region.measurement == active.measurement)
            {
                if ui
                    .selectable_label(region.id == active.id, &region.name)
                    .clicked()
                {
                    selected_region = Some(region.id);
                    ui.close();
                }
            }
        });
    ui.weak(format!("{} points | CPS", active.intensity_cps.len()));

    ui.separator();
    ui.label(crate::typography::headline("Charge correction"));
    charge_controls(app, dataset_id, active, energy_shift_ev, ui);

    ui.separator();
    ui.label(crate::typography::headline("Processing recipe"));
    processing_controls(app, dataset_id, active, recipe, processed, ui);

    if let Some(measurement) = selected_measurement {
        let next = regions
            .iter()
            .filter(|region| region.measurement == measurement)
            .find(|region| region.name.eq_ignore_ascii_case("survey"))
            .or_else(|| {
                regions
                    .iter()
                    .find(|region| region.measurement == measurement)
            });
        if let Some(region) = next {
            report(app.select_xps_region(dataset_id, region.id), app);
        }
    } else if let Some(region) = selected_region {
        report(app.select_xps_region(dataset_id, region), app);
    }
    let _ = dataset_index;
}

fn charge_controls(
    app: &mut PlotxApp,
    dataset: DatasetId,
    active: &plotx_io::xps::XpsRegion,
    energy_shift_ev: f64,
    ui: &mut Ui,
) {
    let shift_key = ui.make_persistent_id(("xps_shift", dataset, active.measurement));
    let mut shift = ui
        .data_mut(|data| data.get_temp::<ShiftEdit>(shift_key))
        .unwrap_or(ShiftEdit {
            value: energy_shift_ev,
            changed: false,
        });
    let response = ui.add_enabled(
        active.binding_energy_ev.is_some(),
        DragValue::new(&mut shift.value)
            .prefix("Shift ")
            .suffix(" eV")
            .speed(0.01),
    );
    shift.changed |= response.changed();
    if (response.drag_stopped() || response.lost_focus()) && shift.changed {
        report(
            app.set_xps_energy_shift(dataset, active.measurement, shift.value),
            app,
        );
        ui.data_mut(|data| data.remove_temp::<ShiftEdit>(shift_key));
    } else {
        ui.data_mut(|data| data.insert_temp(shift_key, shift));
    }

    let reference_key = ui.make_persistent_id(("xps_reference", dataset));
    let mut reference = ui
        .data_mut(|data| data.get_temp::<f64>(reference_key))
        .unwrap_or(284.8);
    ui.horizontal(|ui| {
        ui.label("C 1s reference");
        ui.add(DragValue::new(&mut reference).suffix(" eV").speed(0.01));
    });
    ui.data_mut(|data| data.insert_temp(reference_key, reference));
    let is_c1s = active
        .name
        .to_ascii_lowercase()
        .replace(' ', "")
        .contains("c1s");
    if ui
        .add_enabled(
            is_c1s && active.binding_energy_ev.is_some(),
            Button::new(format!("{} Reference current C 1s", icon::CROSSHAIR)),
        )
        .clicked()
    {
        let result = estimate_xps_charge_shift(
            active.binding_energy_ev.as_deref().unwrap_or_default(),
            &active.intensity_cps,
            reference,
        )
        .and_then(|shift| {
            app.set_xps_energy_shift(dataset, active.measurement, shift)
                .map(|_| shift)
        });
        match result {
            Ok(shift) => {
                ui.data_mut(|data| data.remove_temp::<ShiftEdit>(shift_key));
                app.session.status = format!("Applied {shift:+.2} eV to this position.");
            }
            Err(error) => app.session.status = error,
        }
    }
    ui.weak("The shift applies to every region at this measurement position.");
}

fn processing_controls(
    app: &mut PlotxApp,
    dataset: DatasetId,
    active: &plotx_io::xps::XpsRegion,
    recipe: &plotx_processing::xps::XpsProcessingRecipe,
    processed: Option<&plotx_processing::xps::ProcessedXpsRegion>,
    ui: &mut Ui,
) {
    for (index, step) in recipe.steps.iter().enumerate() {
        let label = match step.kind {
            XpsStepKind::Window { low_ev, high_ev } => {
                format!("Window {low_ev:.2}-{high_ev:.2} eV")
            }
            XpsStepKind::Smooth(_) => "Savitzky-Golay smoothing".into(),
            XpsStepKind::Normalize(_) => "Normalize intensity".into(),
        };
        ui.horizontal(|ui| {
            let mut enabled = step.enabled;
            if ui.checkbox(&mut enabled, "").changed() {
                report(
                    app.set_xps_processing_step_enabled(dataset, active.id, step.id, enabled),
                    app,
                );
            }
            ui.label(label);
            if ui.small_button(icon::ARROW_UP).clicked() && index > 0 {
                report(
                    app.move_xps_processing_step(dataset, active.id, step.id, -1),
                    app,
                );
            }
            if ui.small_button(icon::ARROW_DOWN).clicked() && index + 1 < recipe.steps.len() {
                report(
                    app.move_xps_processing_step(dataset, active.id, step.id, 1),
                    app,
                );
            }
            if ui.small_button(icon::TRASH).clicked() {
                report(
                    app.remove_xps_processing_step(dataset, active.id, step.id),
                    app,
                );
            }
        });
    }
    let extent = processed
        .and_then(|value| {
            Some((
                value.binding_energy_ev.iter().copied().reduce(f64::min)?,
                value.binding_energy_ev.iter().copied().reduce(f64::max)?,
            ))
        })
        .unwrap_or((0.0, 1.0));
    let key = ui.make_persistent_id(("xps_processing_window", dataset, active.id));
    let mut window = ui
        .data_mut(|data| data.get_temp::<WindowDraft>(key))
        .unwrap_or(WindowDraft {
            low_ev: extent.0,
            high_ev: extent.1,
        });
    ui.horizontal(|ui| {
        ui.add(DragValue::new(&mut window.low_ev).speed(0.1));
        ui.label("to");
        ui.add(DragValue::new(&mut window.high_ev).speed(0.1));
        ui.label("eV");
    });
    ui.data_mut(|data| data.insert_temp(key, window.clone()));
    ui.horizontal_wrapped(|ui| {
        if ui.button(format!("{} Window", icon::CROP)).clicked() {
            report(
                app.add_xps_processing_step(
                    dataset,
                    active.id,
                    XpsStepKind::Window {
                        low_ev: window.low_ev,
                        high_ev: window.high_ev,
                    },
                )
                .map(|_| ()),
                app,
            );
        }
        if ui.button(format!("{} Smooth", icon::WAVE_SINE)).clicked() {
            report(
                app.add_xps_processing_step(
                    dataset,
                    active.id,
                    XpsStepKind::Smooth(SmoothMethod::DEFAULT),
                )
                .map(|_| ()),
                app,
            );
        }
        if ui
            .button(format!("{} Normalize", icon::ARROWS_OUT_LINE_VERTICAL))
            .clicked()
        {
            report(
                app.add_xps_processing_step(
                    dataset,
                    active.id,
                    XpsStepKind::Normalize(NormalizeMethod::MaxPeak),
                )
                .map(|_| ()),
                app,
            );
        }
    });
}

pub(super) fn commit_workspace(
    app: &mut PlotxApp,
    dataset_index: usize,
    dataset: DatasetId,
    region: plotx_io::xps::XpsRegionId,
    workspace: XpsFitWorkspace,
    continuous: bool,
    finished: bool,
) {
    if continuous {
        app.begin_processing_session(dataset_index);
    } else {
        app.finish_processing_session();
    }
    report(app.set_xps_fit_workspace(dataset, region, workspace), app);
    if finished {
        app.finish_processing_session();
    }
}

pub(super) fn selected_range(app: &PlotxApp, dataset: DatasetId) -> Option<[f64; 2]> {
    app.session
        .ui
        .analysis_selection
        .as_ref()
        .filter(|selection| selection.dataset == dataset)
        .map(|selection| [selection.x_range.min, selection.x_range.max])
}

pub(super) fn report(result: Result<(), String>, app: &mut PlotxApp) {
    if let Err(error) = result {
        app.session.status = error;
    }
}
