use egui::{Button, DragValue, Ui};
use egui_phosphor::regular as icon;
use plotx_core::state::{DatasetId, PlotxApp, StoredXpsFit};
use plotx_io::xps::XpsRegionId;

pub(super) fn diagnostics_tab(
    app: &mut PlotxApp,
    dataset: DatasetId,
    region: XpsRegionId,
    current: Option<&StoredXpsFit>,
    latest: Option<&StoredXpsFit>,
    ui: &mut Ui,
) {
    ui.label(crate::typography::headline("Fit quality"));
    let Some(fit) = current else {
        if latest.is_some() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "The latest PlotX fit is stale because its input or workspace changed.",
            );
        } else {
            ui.weak("Fit the current workspace to calculate diagnostics.");
        }
        return;
    };
    residual_preview(ui, &fit.result.energy_ev, &fit.result.residual);
    ui.label(format!("R²  {:.6}", fit.result.r_squared));
    ui.label(format!("RMSE  {:.6}", fit.result.rmse));
    if let Some(lag) = fit.result.residual_lag1 {
        ui.label(format!("Residual lag-1  {lag:.4}"));
    }

    let bounds = fit
        .result
        .peaks
        .iter()
        .filter(|peak| peak.hit_position_bound || peak.hit_fwhm_bound || peak.hit_area_bound)
        .count();
    if bounds > 0 {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("{bounds} component(s) reached a parameter bound."),
        );
    }
    let unusual_widths = fit
        .result
        .peaks
        .iter()
        .filter(|peak| !(0.8..=2.5).contains(&peak.fwhm_ev.value))
        .count();
    if unusual_widths > 0 {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("{unusual_widths} FWHM value(s) are outside 0.8-2.5 eV."),
        );
    }
    if let Some(correlation) = &fit.result.parameter_correlation {
        let maximum = correlation
            .iter()
            .enumerate()
            .flat_map(|(row, values)| {
                values
                    .iter()
                    .enumerate()
                    .filter(move |(column, _)| *column != row)
                    .map(|(_, value)| value.abs())
            })
            .reduce(f64::max)
            .unwrap_or(0.0);
        ui.label(format!("Maximum parameter correlation  {maximum:.3}"));
        if maximum > 0.95 {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Strong parameter correlation; interpret individual intervals cautiously.",
            );
        }
    } else {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "Covariance is unavailable because the local matrix is singular.",
        );
    }

    ui.separator();
    ui.label(crate::typography::headline("Parameters"));
    for peak in &fit.result.peaks {
        ui.group(|ui| {
            ui.label(format!("{} · #{}", peak.label, peak.id.0));
            estimate(ui, "Center", &peak.center_ev, " eV");
            estimate(ui, "FWHM", &peak.fwhm_ev, " eV");
            estimate(ui, "Area", &peak.area, "");
            estimate(ui, "Area fraction", &peak.fraction, "");
            if let Some(bootstrap) = fit
                .bootstrap
                .as_ref()
                .and_then(|result| result.peaks.iter().find(|result| result.id == peak.id))
            {
                ui.weak(format!(
                    "Bootstrap center 95%: {:.4} to {:.4} eV",
                    bootstrap.center_ev[0], bootstrap.center_ev[2]
                ));
            }
        });
    }

    ui.separator();
    ui.label(crate::typography::headline("Wild residual Bootstrap"));
    let Some(xps) = app
        .doc
        .datasets
        .iter()
        .find(|candidate| candidate.resource_id() == dataset)
        .and_then(|candidate| candidate.as_xps())
    else {
        return;
    };
    let Some(mut workspace) = xps.fit_workspaces.get(&region).cloned() else {
        return;
    };
    let dataset_index = app.doc.dataset_index(dataset).unwrap_or_default();
    let mut changed = false;
    let samples = ui.add(
        DragValue::new(&mut workspace.bootstrap.samples)
            .prefix("Replicates ")
            .range(100..=5_000),
    );
    changed |= samples.changed();
    let seed = ui.add(DragValue::new(&mut workspace.bootstrap.seed).prefix("Seed (0 = auto) "));
    changed |= seed.changed();
    if changed {
        super::commit_workspace(
            app,
            dataset_index,
            dataset,
            region,
            workspace,
            true,
            samples.drag_stopped()
                || samples.lost_focus()
                || seed.drag_stopped()
                || seed.lost_focus(),
        );
    }
    if let Some(result) = &fit.bootstrap {
        ui.label(format!(
            "{} of {} replicates converged ({:.0}%), seed {}",
            result.converged,
            result.requested,
            result.convergence_fraction() * 100.0,
            result.seed
        ));
        if result.convergence_fraction() < 0.8 {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Bootstrap convergence is below 80%; intervals were retained but may be unstable.",
            );
        }
    }
    let progress = app
        .xps_fit_progress()
        .filter(|(owner, active, _)| *owner == dataset && *active == region);
    if let Some((_, _, elapsed)) = progress {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(format!("Analyzing... {:.1} s", elapsed.as_secs_f64()));
            if ui.button(format!("{} Cancel", icon::X)).clicked() {
                app.cancel_xps_fit();
            }
        });
    } else if ui
        .add(Button::new(format!("{} Run Bootstrap", icon::PLAY)))
        .clicked()
        && let Err(error) = app.start_xps_bootstrap(dataset, region)
    {
        app.session.status = error;
    }
    ui.weak("Intervals are diagnostic; R² alone does not establish chemical assignment.");
}

fn residual_preview(ui: &mut Ui, energy: &[f64], residual: &[f64]) {
    let width = ui.available_width().max(80.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 88.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        ui.visuals().widgets.noninteractive.corner_radius,
        ui.visuals().faint_bg_color,
    );
    let Some((energy_min, energy_max)) = finite_extent(energy) else {
        return;
    };
    let maximum = residual
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .map(f64::abs)
        .fold(0.0_f64, f64::max);
    if energy_max <= energy_min || maximum <= f64::EPSILON {
        return;
    }
    let plot = rect.shrink2(egui::vec2(5.0, 7.0));
    painter.line_segment(
        [
            egui::pos2(plot.left(), plot.center().y),
            egui::pos2(plot.right(), plot.center().y),
        ],
        egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    let points = energy
        .iter()
        .copied()
        .zip(residual.iter().copied())
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(x, y)| {
            let tx = ((energy_max - x) / (energy_max - energy_min)) as f32;
            let ty = (y / maximum) as f32;
            egui::pos2(
                plot.left() + tx * plot.width(),
                plot.center().y - ty * plot.height() * 0.5,
            )
        })
        .collect::<Vec<_>>();
    if points.len() >= 2 {
        painter.add(egui::Shape::line(
            points,
            egui::Stroke::new(1.2_f32, egui::Color32::from_rgb(0xae, 0x2c, 0x2c)),
        ));
    }
}

fn finite_extent(values: &[f64]) -> Option<(f64, f64)> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for value in values.iter().copied().filter(|value| value.is_finite()) {
        min = min.min(value);
        max = max.max(value);
    }
    min.is_finite().then_some((min, max))
}

fn estimate(
    ui: &mut Ui,
    label: &str,
    estimate: &plotx_analysis::xps::XpsParameterEstimate,
    suffix: &str,
) {
    if let Some(interval) = estimate.confidence_95 {
        ui.label(format!(
            "{label}: {:.5}{suffix} (95% {:.5} to {:.5})",
            estimate.value, interval[0], interval[1]
        ));
    } else {
        ui.label(format!("{label}: {:.5}{suffix}", estimate.value));
    }
}
