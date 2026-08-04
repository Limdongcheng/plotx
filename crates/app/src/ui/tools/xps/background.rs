use super::{commit_workspace, selected_range};
use egui::{Button, ComboBox, DragValue, Ui};
use egui_phosphor::regular as icon;
use plotx_analysis::xps::{XpsBackgroundModel, compute_xps_background};
use plotx_core::state::{DatasetId, PlotxApp, Tool, XpsFitWorkspace};

pub(super) fn background_tab(
    app: &mut PlotxApp,
    dataset_index: usize,
    dataset: DatasetId,
    region: &plotx_io::xps::XpsRegion,
    processed: Option<&plotx_processing::xps::ProcessedXpsRegion>,
    mut workspace: XpsFitWorkspace,
    ui: &mut Ui,
) {
    ui.label(crate::typography::headline("Background model"));
    let before_model = model_kind(&workspace.invocation.background.model);
    let mut kind = before_model;
    ComboBox::from_label("Model")
        .selected_text(kind.label())
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut kind, ModelKind::Linear, "Linear");
            ui.selectable_value(&mut kind, ModelKind::Shirley, "Shirley");
            ui.selectable_value(&mut kind, ModelKind::Tougaard, "Tougaard U2");
        });
    if kind != before_model {
        workspace.invocation.background.model = kind.default_model();
        commit_workspace(
            app,
            dataset_index,
            dataset,
            region.id,
            workspace.clone(),
            false,
            true,
        );
    }

    let mut changed = false;
    let mut finished = false;
    match &mut workspace.invocation.background.model {
        XpsBackgroundModel::Linear => {}
        XpsBackgroundModel::Shirley {
            tolerance,
            max_iterations,
        } => {
            ui.horizontal(|ui| {
                ui.label("Tolerance");
                let response = ui.add(DragValue::new(tolerance).speed(1e-6).range(1e-12..=1.0));
                changed |= response.changed();
                finished |= response.drag_stopped() || response.lost_focus();
            });
            ui.horizontal(|ui| {
                ui.label("Max iterations");
                let response = ui.add(DragValue::new(max_iterations).range(1..=100_000));
                changed |= response.changed();
                finished |= response.drag_stopped() || response.lost_focus();
            });
        }
        XpsBackgroundModel::TougaardU2 { b_ev2, c_ev2 } => {
            ui.horizontal(|ui| {
                ui.label("B");
                let response = ui.add(
                    DragValue::new(b_ev2)
                        .suffix(" eV²")
                        .speed(10.0)
                        .range(0.0..=f64::INFINITY),
                );
                changed |= response.changed();
                finished |= response.drag_stopped() || response.lost_focus();
            });
            ui.horizontal(|ui| {
                ui.label("C");
                let response = ui.add(
                    DragValue::new(c_ev2)
                        .suffix(" eV²")
                        .speed(10.0)
                        .range(f64::MIN_POSITIVE..=f64::INFINITY),
                );
                changed |= response.changed();
                finished |= response.drag_stopped() || response.lost_focus();
            });
        }
    }

    ui.separator();
    ui.label(crate::typography::headline("Fit window and anchors"));
    let range = selected_range(app, dataset);
    ui.horizontal_wrapped(|ui| {
        let selecting = app.session.tool == Tool::SelectRegion;
        if ui.selectable_label(selecting, "Select on plot").clicked() {
            app.toggle_tool(Tool::SelectRegion);
        }
        if ui
            .add_enabled(
                range.is_some(),
                Button::new(format!("{} Set window", icon::CROP)),
            )
            .clicked()
        {
            workspace.invocation.background.window_ev = range.unwrap_or_default();
            commit_workspace(
                app,
                dataset_index,
                dataset,
                region.id,
                workspace.clone(),
                false,
                true,
            );
        }
        if ui
            .add_enabled(range.is_some(), Button::new("Set low anchor"))
            .clicked()
        {
            workspace.invocation.background.low_anchor_ev = range.unwrap_or_default();
            commit_workspace(
                app,
                dataset_index,
                dataset,
                region.id,
                workspace.clone(),
                false,
                true,
            );
        }
        if ui
            .add_enabled(range.is_some(), Button::new("Set high anchor"))
            .clicked()
        {
            workspace.invocation.background.high_anchor_ev = range.unwrap_or_default();
            commit_workspace(
                app,
                dataset_index,
                dataset,
                region.id,
                workspace.clone(),
                false,
                true,
            );
        }
    });
    changed |= range_editor(
        ui,
        "Fit window",
        &mut workspace.invocation.background.window_ev,
        &mut finished,
    );
    changed |= range_editor(
        ui,
        "Low-BE anchor",
        &mut workspace.invocation.background.low_anchor_ev,
        &mut finished,
    );
    changed |= range_editor(
        ui,
        "High-BE anchor",
        &mut workspace.invocation.background.high_anchor_ev,
        &mut finished,
    );
    if changed {
        commit_workspace(
            app,
            dataset_index,
            dataset,
            region.id,
            workspace.clone(),
            true,
            finished,
        );
    }

    ui.separator();
    ui.label(crate::typography::headline("Preview"));
    if let Some(processed) = processed {
        match compute_xps_background(
            &processed.binding_energy_ev,
            &processed.intensity,
            &workspace.invocation.background,
        ) {
            Ok(preview) => {
                let corrected = preview
                    .corrected
                    .iter()
                    .copied()
                    .reduce(f64::max)
                    .unwrap_or(0.0);
                ui.label(format!("{} points in fit window", preview.energy_ev.len()));
                ui.weak(format!(
                    "Maximum background-subtracted intensity: {corrected:.4}"
                ));
                ui.weak("The plot shows the live background and background-subtracted trace.");
            }
            Err(error) => {
                ui.colored_label(ui.visuals().error_fg_color, error.to_string());
            }
        }
    }
}

fn range_editor(ui: &mut Ui, label: &str, range: &mut [f64; 2], finished: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        for value in range {
            let response = ui.add(DragValue::new(value).suffix(" eV").speed(0.05));
            changed |= response.changed();
            *finished |= response.drag_stopped() || response.lost_focus();
        }
    });
    changed
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModelKind {
    Linear,
    Shirley,
    Tougaard,
}

impl ModelKind {
    fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Shirley => "Shirley",
            Self::Tougaard => "Tougaard U2",
        }
    }

    fn default_model(self) -> XpsBackgroundModel {
        match self {
            Self::Linear => XpsBackgroundModel::Linear,
            Self::Shirley => XpsBackgroundModel::default(),
            Self::Tougaard => XpsBackgroundModel::TougaardU2 {
                b_ev2: 3000.0,
                c_ev2: 1643.0,
            },
        }
    }
}

fn model_kind(model: &XpsBackgroundModel) -> ModelKind {
    match model {
        XpsBackgroundModel::Linear => ModelKind::Linear,
        XpsBackgroundModel::Shirley { .. } => ModelKind::Shirley,
        XpsBackgroundModel::TougaardU2 { .. } => ModelKind::Tougaard,
    }
}
