use super::commit_workspace;
use egui::{Button, ComboBox, DragValue, Ui};
use egui_phosphor::regular as icon;
use plotx_analysis::xps::{
    XpsAreaConstraint, XpsCenterConstraint, XpsComponentId, XpsFwhmConstraint, XpsPeakSpec,
};
use plotx_core::state::{DatasetId, PlotxApp, XpsFitWorkspace, xps_template};

pub(super) fn components_tab(
    app: &mut PlotxApp,
    dataset_index: usize,
    dataset: DatasetId,
    region: &plotx_io::xps::XpsRegion,
    processed: Option<&plotx_processing::xps::ProcessedXpsRegion>,
    mut workspace: XpsFitWorkspace,
    ui: &mut Ui,
) {
    if let Some(imported) = &region.imported_fit {
        ui.label(format!(
            "Imported (CasaXPS): {} components",
            imported.peaks.len()
        ));
        if ui
            .button(format!("{} Copy Imported fit", icon::COPY))
            .clicked()
        {
            workspace.invocation.peaks = imported
                .peaks
                .iter()
                .map(|peak| {
                    let id = allocate_id(&mut workspace);
                    let mut spec = XpsPeakSpec::independent(
                        id,
                        peak.label.clone(),
                        peak.position_ev,
                        peak.area,
                    );
                    spec.fwhm = XpsFwhmConstraint::Free {
                        initial_ev: peak.fwhm_ev.max(0.1),
                        bounds_ev: [0.2, 5.0],
                    };
                    spec
                })
                .collect();
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
    }

    ui.horizontal_wrapped(|ui| {
        let mut next = workspace.next_component_id;
        let template = xps_template(
            &region.name,
            processed.map_or(&[], |value| value.intensity.as_slice()),
            &mut next,
        );
        if ui
            .add_enabled(
                template.is_some(),
                Button::new(format!("{} Use template", icon::LIST_PLUS)),
            )
            .clicked()
        {
            workspace.invocation.peaks = template.unwrap_or_default();
            workspace.next_component_id = next;
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
        if ui.button(format!("{} Add component", icon::PLUS)).clicked() {
            let center = processed
                .and_then(|value| {
                    Some(
                        0.5 * (value.binding_energy_ev.iter().copied().reduce(f64::min)?
                            + value.binding_energy_ev.iter().copied().reduce(f64::max)?),
                    )
                })
                .unwrap_or(0.0);
            let area = processed.map_or(1.0, |value| {
                value
                    .intensity
                    .iter()
                    .copied()
                    .reduce(f64::max)
                    .unwrap_or(1.0)
                    .max(1.0)
            });
            let id = allocate_id(&mut workspace);
            let label = format!("Peak {}", workspace.invocation.peaks.len() + 1);
            workspace
                .invocation
                .peaks
                .push(XpsPeakSpec::independent(id, label, center, area));
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

    let choices = workspace
        .invocation
        .peaks
        .iter()
        .map(|peak| (peak.id, peak.label.clone()))
        .collect::<Vec<_>>();
    let referenced_ids = choices
        .iter()
        .filter_map(|(id, _)| is_referenced(*id, &workspace.invocation.peaks).then_some(*id))
        .collect::<Vec<_>>();
    let mut changed = false;
    let mut finished = false;
    let mut operation = None;
    let count = workspace.invocation.peaks.len();
    for (index, peak) in workspace.invocation.peaks.iter_mut().enumerate() {
        let referenced = referenced_ids.contains(&peak.id);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Component {} · #{}", index + 1, peak.id.0));
                if ui
                    .small_button(icon::ARROW_UP)
                    .on_hover_text("Move up")
                    .clicked()
                    && index > 0
                {
                    operation = Some(Operation::Move(index, index - 1));
                }
                if ui
                    .small_button(icon::ARROW_DOWN)
                    .on_hover_text("Move down")
                    .clicked()
                    && index + 1 < count
                {
                    operation = Some(Operation::Move(index, index + 1));
                }
                if ui
                    .small_button(icon::COPY)
                    .on_hover_text("Copy as linked component")
                    .clicked()
                {
                    operation = Some(Operation::CopyLinked(index));
                }
                if ui
                    .add_enabled(!referenced, Button::new(icon::TRASH))
                    .on_disabled_hover_text("Another component references this component.")
                    .clicked()
                {
                    operation = Some(Operation::Remove(index));
                }
            });
            let response = ui.text_edit_singleline(&mut peak.label);
            changed |= response.changed();
            finished |= response.lost_focus();
            changed |= center_editor(ui, peak.id, &mut peak.center, &choices, &mut finished);
            changed |= fwhm_editor(ui, peak.id, &mut peak.fwhm, &choices, &mut finished);
            changed |= area_editor(ui, peak.id, &mut peak.area, &choices, &mut finished);
        });
    }

    if let Some(operation) = operation {
        match operation {
            Operation::Move(from, to) => workspace.invocation.peaks.swap(from, to),
            Operation::Remove(index) => {
                workspace.invocation.peaks.remove(index);
            }
            Operation::CopyLinked(index) => {
                let reference = workspace.invocation.peaks[index].id;
                let id = allocate_id(&mut workspace);
                workspace.invocation.peaks.push(XpsPeakSpec {
                    id,
                    label: format!("{} linked", workspace.invocation.peaks[index].label),
                    center: XpsCenterConstraint::Offset {
                        reference,
                        delta_ev: 1.0,
                    },
                    fwhm: XpsFwhmConstraint::Shared { reference },
                    area: XpsAreaConstraint::Ratio {
                        reference,
                        ratio: 0.5,
                    },
                });
            }
        }
        commit_workspace(
            app,
            dataset_index,
            dataset,
            region.id,
            workspace.clone(),
            false,
            true,
        );
    } else if changed {
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
    ui.label(crate::typography::headline("Fit options"));
    let response = ui.add(
        DragValue::new(&mut workspace.invocation.options.lorentzian_fraction)
            .prefix("GL Lorentzian fraction ")
            .range(0.0..=1.0)
            .speed(0.01),
    );
    if response.changed() {
        commit_workspace(
            app,
            dataset_index,
            dataset,
            region.id,
            workspace.clone(),
            true,
            response.drag_stopped() || response.lost_focus(),
        );
    }
    let progress = app
        .xps_fit_progress()
        .filter(|(owner, active, _)| *owner == dataset && *active == region.id);
    if let Some((_, _, elapsed)) = progress {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(format!("Analyzing... {:.1} s", elapsed.as_secs_f64()));
            if ui.button(format!("{} Cancel", icon::X)).clicked() {
                app.cancel_xps_fit();
            }
        });
    }
    if ui
        .add_enabled(
            !workspace.invocation.peaks.is_empty() && processed.is_some() && progress.is_none(),
            Button::new(format!("{} Fit peaks", icon::PLAY)),
        )
        .on_disabled_hover_text("Add at least one component first.")
        .clicked()
        && let Err(error) = app.start_xps_fit(dataset, region.id)
    {
        app.session.status = error;
    }
}

fn center_editor(
    ui: &mut Ui,
    owner: XpsComponentId,
    value: &mut XpsCenterConstraint,
    choices: &[(XpsComponentId, String)],
    finished: &mut bool,
) -> bool {
    let mut mode = match value {
        XpsCenterConstraint::Free { .. } => 0,
        XpsCenterConstraint::Fixed { .. } => 1,
        XpsCenterConstraint::Offset { .. } => 2,
    };
    let before = mode;
    ComboBox::from_id_salt(("center_mode", owner.0))
        .selected_text(["Free + bounded", "Fixed", "Energy offset"][mode])
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut mode, 0, "Free + bounded");
            ui.selectable_value(&mut mode, 1, "Fixed");
            ui.selectable_value(&mut mode, 2, "Energy offset");
        });
    if mode != before {
        *value = default_center(mode, value, owner, choices);
        return true;
    }
    let mut changed = false;
    match value {
        XpsCenterConstraint::Free {
            initial_ev,
            bounds_ev,
        } => {
            let [low, high] = bounds_ev;
            changed |= values(ui, "Center / bounds", [initial_ev, low, high], finished);
        }
        XpsCenterConstraint::Fixed { value_ev } => {
            changed |= values(ui, "Center", [value_ev], finished)
        }
        XpsCenterConstraint::Offset {
            reference,
            delta_ev,
        } => {
            changed |= reference_picker(ui, ("center_ref", owner.0), owner, reference, choices);
            changed |= values(ui, "Energy difference", [delta_ev], finished);
        }
    }
    changed
}

fn fwhm_editor(
    ui: &mut Ui,
    owner: XpsComponentId,
    value: &mut XpsFwhmConstraint,
    choices: &[(XpsComponentId, String)],
    finished: &mut bool,
) -> bool {
    let mut mode = match value {
        XpsFwhmConstraint::Free { .. } => 0,
        XpsFwhmConstraint::Fixed { .. } => 1,
        XpsFwhmConstraint::Shared { .. } => 2,
    };
    let before = mode;
    ComboBox::from_id_salt(("fwhm_mode", owner.0))
        .selected_text(["Free + bounded", "Fixed", "Shared width"][mode])
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut mode, 0, "Free + bounded");
            ui.selectable_value(&mut mode, 1, "Fixed");
            ui.selectable_value(&mut mode, 2, "Shared width");
        });
    if mode != before {
        *value = default_fwhm(mode, owner, choices);
        return true;
    }
    match value {
        XpsFwhmConstraint::Free {
            initial_ev,
            bounds_ev,
        } => {
            let [low, high] = bounds_ev;
            values(ui, "FWHM / bounds", [initial_ev, low, high], finished)
        }
        XpsFwhmConstraint::Fixed { value_ev } => values(ui, "FWHM", [value_ev], finished),
        XpsFwhmConstraint::Shared { reference } => {
            reference_picker(ui, ("fwhm_ref", owner.0), owner, reference, choices)
        }
    }
}

fn area_editor(
    ui: &mut Ui,
    owner: XpsComponentId,
    value: &mut XpsAreaConstraint,
    choices: &[(XpsComponentId, String)],
    finished: &mut bool,
) -> bool {
    let mut mode = match value {
        XpsAreaConstraint::Free { .. } => 0,
        XpsAreaConstraint::Fixed { .. } => 1,
        XpsAreaConstraint::Ratio { .. } => 2,
    };
    let before = mode;
    ComboBox::from_id_salt(("area_mode", owner.0))
        .selected_text(["Free nonnegative", "Fixed", "Area ratio"][mode])
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut mode, 0, "Free nonnegative");
            ui.selectable_value(&mut mode, 1, "Fixed");
            ui.selectable_value(&mut mode, 2, "Area ratio");
        });
    if mode != before {
        *value = default_area(mode, owner, choices);
        return true;
    }
    let mut changed = false;
    match value {
        XpsAreaConstraint::Free { initial, bounds } => {
            let [low, high] = bounds;
            changed |= values(ui, "Area / bounds", [initial, low, high], finished);
        }
        XpsAreaConstraint::Fixed { value } => changed |= values(ui, "Area", [value], finished),
        XpsAreaConstraint::Ratio { reference, ratio } => {
            changed |= reference_picker(ui, ("area_ref", owner.0), owner, reference, choices);
            changed |= values(ui, "Area ratio", [ratio], finished);
        }
    }
    changed
}

fn values<const N: usize>(
    ui: &mut Ui,
    label: &str,
    values: [&mut f64; N],
    finished: &mut bool,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        for value in values {
            let response = ui.add(DragValue::new(value).speed(0.01));
            changed |= response.changed();
            *finished |= response.drag_stopped() || response.lost_focus();
        }
    });
    changed
}

fn reference_picker(
    ui: &mut Ui,
    salt: impl std::hash::Hash,
    owner: XpsComponentId,
    reference: &mut XpsComponentId,
    choices: &[(XpsComponentId, String)],
) -> bool {
    let before = *reference;
    let label = choices
        .iter()
        .find(|(id, _)| id == reference)
        .map_or("Choose component", |(_, label)| label);
    ComboBox::from_id_salt(salt)
        .selected_text(label)
        .show_ui(ui, |ui| {
            for (id, label) in choices.iter().filter(|(id, _)| *id != owner) {
                ui.selectable_value(reference, *id, label);
            }
        });
    *reference != before
}

fn other(owner: XpsComponentId, choices: &[(XpsComponentId, String)]) -> XpsComponentId {
    choices
        .iter()
        .find(|(id, _)| *id != owner)
        .map_or(owner, |(id, _)| *id)
}
fn default_center(
    mode: usize,
    old: &XpsCenterConstraint,
    owner: XpsComponentId,
    choices: &[(XpsComponentId, String)],
) -> XpsCenterConstraint {
    let center = match old {
        XpsCenterConstraint::Free { initial_ev, .. } => *initial_ev,
        XpsCenterConstraint::Fixed { value_ev } => *value_ev,
        XpsCenterConstraint::Offset { delta_ev, .. } => *delta_ev,
    };
    match mode {
        0 => XpsCenterConstraint::Free {
            initial_ev: center,
            bounds_ev: [center - 0.8, center + 0.8],
        },
        1 => XpsCenterConstraint::Fixed { value_ev: center },
        _ => XpsCenterConstraint::Offset {
            reference: other(owner, choices),
            delta_ev: 1.0,
        },
    }
}
fn default_fwhm(
    mode: usize,
    owner: XpsComponentId,
    choices: &[(XpsComponentId, String)],
) -> XpsFwhmConstraint {
    match mode {
        0 => XpsFwhmConstraint::Free {
            initial_ev: 1.2,
            bounds_ev: [0.8, 2.5],
        },
        1 => XpsFwhmConstraint::Fixed { value_ev: 1.2 },
        _ => XpsFwhmConstraint::Shared {
            reference: other(owner, choices),
        },
    }
}
fn default_area(
    mode: usize,
    owner: XpsComponentId,
    choices: &[(XpsComponentId, String)],
) -> XpsAreaConstraint {
    match mode {
        0 => XpsAreaConstraint::Free {
            initial: 1.0,
            bounds: [0.0, 20.0],
        },
        1 => XpsAreaConstraint::Fixed { value: 1.0 },
        _ => XpsAreaConstraint::Ratio {
            reference: other(owner, choices),
            ratio: 0.5,
        },
    }
}

fn allocate_id(workspace: &mut XpsFitWorkspace) -> XpsComponentId {
    let id = XpsComponentId::new(workspace.next_component_id);
    workspace.next_component_id = workspace.next_component_id.saturating_add(1);
    id
}
fn is_referenced(id: XpsComponentId, peaks: &[XpsPeakSpec]) -> bool {
    peaks.iter().any(|peak| {
        matches!(peak.center, XpsCenterConstraint::Offset { reference, .. } if reference == id)
            || matches!(peak.fwhm, XpsFwhmConstraint::Shared { reference } if reference == id)
            || matches!(peak.area, XpsAreaConstraint::Ratio { reference, .. } if reference == id)
    })
}

enum Operation {
    Move(usize, usize),
    Remove(usize),
    CopyLinked(usize),
}
