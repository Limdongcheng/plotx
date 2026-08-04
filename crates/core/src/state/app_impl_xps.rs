use super::{DatasetId, PlotxApp, StoredXpsFit, XpsFitWorkspace};
use crate::actions::{Action, DatasetProcessingState};
use plotx_analysis::xps::{
    XpsBootstrapOptions, XpsBootstrapResult, XpsComponentId, XpsFitError, XpsFitInvocation,
    XpsFitResult, XpsPeakSpec, bootstrap_xps_fit, fit_xps_peaks,
};
use plotx_io::xps::{XpsMeasurementId, XpsRegionId};
use plotx_processing::StepId;
use plotx_processing::xps::{XpsProcessingStep, XpsStepKind};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

pub struct XpsFitWorker {
    dataset: DatasetId,
    epoch: u64,
    region: XpsRegionId,
    input_sha256: String,
    energy_shift_ev: f64,
    processing_recipe: plotx_processing::xps::XpsProcessingRecipe,
    invocation: XpsFitInvocation,
    started_at: Instant,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<XpsFitResult, XpsFitError>>,
}

pub struct XpsBootstrapWorker {
    dataset: DatasetId,
    epoch: u64,
    region: XpsRegionId,
    input_sha256: String,
    options: XpsBootstrapOptions,
    started_at: Instant,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<XpsBootstrapResult, XpsFitError>>,
}

pub enum XpsFitJob {
    Fit(XpsFitWorker),
    Bootstrap(XpsBootstrapWorker),
}

impl XpsFitJob {
    fn common(&self) -> (DatasetId, XpsRegionId, Instant, &Arc<AtomicBool>) {
        match self {
            Self::Fit(job) => (job.dataset, job.region, job.started_at, &job.cancel),
            Self::Bootstrap(job) => (job.dataset, job.region, job.started_at, &job.cancel),
        }
    }
}

impl PlotxApp {
    fn edit_xps_state(
        &mut self,
        dataset: DatasetId,
        edit: impl FnOnce(&mut DatasetProcessingState) -> Result<(), String>,
    ) -> Result<(), String> {
        let index = self
            .doc
            .dataset_index(dataset)
            .ok_or_else(|| "The XPS dataset is no longer available.".to_owned())?;
        let before = DatasetProcessingState::from_dataset(&self.doc.datasets[index]);
        let mut after = before.clone();
        edit(&mut after)?;
        self.try_execute_action(Action::update_dataset_processing(dataset, before, after))
            .map_err(|error| error.to_string())
    }

    pub fn select_xps_region(
        &mut self,
        dataset: DatasetId,
        region: XpsRegionId,
    ) -> Result<(), String> {
        let index = self
            .doc
            .dataset_index(dataset)
            .ok_or_else(|| "The XPS dataset is no longer available.".to_owned())?;
        let new_field = {
            let xps = self.doc.datasets[index]
                .as_xps()
                .ok_or_else(|| "The selected dataset is not XPS data.".to_owned())?;
            if xps.region(region).is_none() {
                return Err("The selected XPS region is no longer available.".into());
            }
            xps.field_for_region(region)
                .ok_or_else(|| "The selected XPS region has no plot field.".to_owned())?
        };
        let before = DatasetProcessingState::from_dataset(&self.doc.datasets[index]);
        let mut after = before.clone();
        let DatasetProcessingState::Xps { active_region, .. } = &mut after else {
            return Err("The selected dataset is not XPS data.".into());
        };
        *active_region = region;

        let mut actions = vec![Action::update_dataset_processing(dataset, before, after)];
        if let Some(canvas_index) = self.session.active_canvas
            && let Some(canvas) = self.doc.canvases.get(canvas_index)
            && let Some(object_id) = canvas.selected_plot_object_id()
            && let Some(plot) = canvas.object(object_id).and_then(|object| object.plot())
        {
            let before = plot.binding.clone();
            let mut after = before.clone();
            let candidate = after
                .series
                .iter()
                .position(|series| series.source.resource == dataset);
            if let Some(series) = candidate {
                after.series[series].source.field = new_field;
                actions.push(Action::set_data_binding(
                    canvas_index,
                    object_id,
                    before,
                    after,
                ));
            }
        }
        self.try_execute_action(Action::Composite(actions))
            .map_err(|error| error.to_string())
    }

    pub fn set_xps_energy_shift(
        &mut self,
        dataset: DatasetId,
        measurement: XpsMeasurementId,
        shift_ev: f64,
    ) -> Result<(), String> {
        if !shift_ev.is_finite() {
            return Err("The XPS energy shift must be finite.".into());
        }
        let index = self
            .doc
            .dataset_index(dataset)
            .ok_or_else(|| "The XPS dataset is no longer available.".to_owned())?;
        let xps = self.doc.datasets[index]
            .as_xps()
            .ok_or_else(|| "The selected dataset is not XPS data.".to_owned())?;
        let old_shift = xps
            .energy_shift(measurement)
            .ok_or_else(|| "The XPS measurement is no longer available.".to_owned())?;
        let affected = xps
            .experiment
            .regions
            .iter()
            .filter(|region| region.measurement == measurement)
            .map(|region| region.id)
            .collect::<Vec<_>>();
        let delta = shift_ev - old_shift;
        self.edit_xps_state(dataset, |state| match state {
            DatasetProcessingState::Xps {
                measurement_shifts,
                region_recipes,
                fit_workspaces,
                ..
            } => {
                *measurement_shifts
                    .get_mut(&measurement)
                    .ok_or_else(|| "The XPS measurement is no longer available.".to_owned())? =
                    shift_ev;
                // Selection ranges stay on the same sampled points; component
                // centers remain absolute chemical binding energies.
                for region in affected {
                    if let Some(recipe) = region_recipes.get_mut(&region) {
                        shift_processing_windows(recipe, delta);
                    }
                    if let Some(workspace) = fit_workspaces.get_mut(&region) {
                        shift_background_ranges(workspace, delta);
                    }
                }
                Ok(())
            }
            _ => Err("The selected dataset is not XPS data.".into()),
        })
    }

    pub fn set_xps_fit_workspace(
        &mut self,
        dataset: DatasetId,
        region: XpsRegionId,
        workspace: XpsFitWorkspace,
    ) -> Result<(), String> {
        let index = self
            .doc
            .dataset_index(dataset)
            .ok_or_else(|| "The XPS dataset is no longer available.".to_owned())?;
        let before = DatasetProcessingState::from_dataset(&self.doc.datasets[index]);
        let mut after = before.clone();
        match &mut after {
            DatasetProcessingState::Xps { fit_workspaces, .. } => {
                if !fit_workspaces.contains_key(&region) {
                    return Err("This XPS region has no binding-energy fitting workspace.".into());
                }
                fit_workspaces.insert(region, workspace);
            }
            _ => return Err("The selected dataset is not XPS data.".into()),
        }
        self.try_commit_processing_edit(index, before, after)
    }

    pub fn add_xps_processing_step(
        &mut self,
        dataset: DatasetId,
        region: XpsRegionId,
        kind: XpsStepKind,
    ) -> Result<StepId, String> {
        let mut assigned = None;
        self.edit_xps_state(dataset, |state| match state {
            DatasetProcessingState::Xps {
                region_recipes,
                next_step_id,
                ..
            } => {
                let id = StepId::new(*next_step_id);
                *next_step_id = next_step_id.saturating_add(1);
                region_recipes
                    .get_mut(&region)
                    .ok_or_else(|| "The XPS region is no longer available.".to_owned())?
                    .steps
                    .push(XpsProcessingStep {
                        id,
                        kind,
                        enabled: true,
                        source: plotx_processing::StepSource::User,
                    });
                assigned = Some(id);
                Ok(())
            }
            _ => Err("The selected dataset is not XPS data.".into()),
        })?;
        assigned.ok_or_else(|| "The XPS step could not be created.".into())
    }

    pub fn remove_xps_processing_step(
        &mut self,
        dataset: DatasetId,
        region: XpsRegionId,
        step: StepId,
    ) -> Result<(), String> {
        self.edit_xps_state(dataset, |state| match state {
            DatasetProcessingState::Xps { region_recipes, .. } => {
                let steps = &mut region_recipes
                    .get_mut(&region)
                    .ok_or_else(|| "The XPS region is no longer available.".to_owned())?
                    .steps;
                let before = steps.len();
                steps.retain(|candidate| candidate.id != step);
                if steps.len() == before {
                    return Err("The XPS processing step is no longer available.".into());
                }
                Ok(())
            }
            _ => Err("The selected dataset is not XPS data.".into()),
        })
    }

    pub fn set_xps_processing_step_enabled(
        &mut self,
        dataset: DatasetId,
        region: XpsRegionId,
        step: StepId,
        enabled: bool,
    ) -> Result<(), String> {
        self.edit_xps_state(dataset, |state| match state {
            DatasetProcessingState::Xps { region_recipes, .. } => {
                let candidate = region_recipes
                    .get_mut(&region)
                    .and_then(|recipe| {
                        recipe
                            .steps
                            .iter_mut()
                            .find(|candidate| candidate.id == step)
                    })
                    .ok_or_else(|| "The XPS processing step is no longer available.".to_owned())?;
                candidate.enabled = enabled;
                Ok(())
            }
            _ => Err("The selected dataset is not XPS data.".into()),
        })
    }

    pub fn move_xps_processing_step(
        &mut self,
        dataset: DatasetId,
        region: XpsRegionId,
        step: StepId,
        offset: isize,
    ) -> Result<(), String> {
        self.edit_xps_state(dataset, |state| match state {
            DatasetProcessingState::Xps { region_recipes, .. } => {
                let steps = &mut region_recipes
                    .get_mut(&region)
                    .ok_or_else(|| "The XPS region is no longer available.".to_owned())?
                    .steps;
                let index = steps
                    .iter()
                    .position(|candidate| candidate.id == step)
                    .ok_or_else(|| "The XPS processing step is no longer available.".to_owned())?;
                let target = index
                    .saturating_add_signed(offset)
                    .min(steps.len().saturating_sub(1));
                if target != index {
                    steps.swap(index, target);
                }
                Ok(())
            }
            _ => Err("The selected dataset is not XPS data.".into()),
        })
    }

    pub fn run_xps_fit(&mut self, dataset: DatasetId, region: XpsRegionId) -> Result<(), String> {
        let index = self
            .doc
            .dataset_index(dataset)
            .ok_or_else(|| "The XPS dataset is no longer available.".to_owned())?;
        let xps = self.doc.datasets[index]
            .as_xps()
            .ok_or_else(|| "The selected dataset is not XPS data.".to_owned())?;
        let processed = xps.processed_region(region).ok_or_else(|| {
            "This region has no binding-energy axis, so fitting is unavailable.".to_owned()
        })?;
        let measurement = xps
            .region(region)
            .ok_or_else(|| "The XPS region is no longer available.".to_owned())?
            .measurement;
        let processing_recipe = xps
            .recipe(region)
            .cloned()
            .ok_or_else(|| "This XPS region has no processing recipe.".to_owned())?;
        let energy_shift_ev = xps
            .energy_shift(measurement)
            .ok_or_else(|| "This XPS measurement has no energy shift.".to_owned())?;
        let invocation = xps
            .fit_workspaces
            .get(&region)
            .ok_or_else(|| "This XPS region has no fitting workspace.".to_owned())?
            .invocation
            .clone();
        let input_sha256 = xps_input_sha256(
            region,
            &processed.binding_energy_ev,
            &processed.intensity,
            &invocation,
        );
        let result = fit_xps_peaks(
            &processed.binding_energy_ev,
            &processed.intensity,
            &invocation,
            &|| false,
        )
        .map_err(xps_fit_error)?;
        let stored = StoredXpsFit {
            region,
            input_sha256,
            energy_shift_ev,
            processing_recipe,
            invocation,
            result,
            bootstrap: None,
        };
        self.edit_xps_state(dataset, |state| match state {
            DatasetProcessingState::Xps { fits, .. } => {
                fits.entry(region).or_default().push(stored);
                Ok(())
            }
            _ => Err("The selected dataset is not XPS data.".into()),
        })
    }

    pub fn start_xps_fit(&mut self, dataset: DatasetId, region: XpsRegionId) -> Result<(), String> {
        if self.session.xps_fit_job.is_some() {
            return Err("An XPS fit is already running; cancel it or wait for completion.".into());
        }
        let index = self
            .doc
            .dataset_index(dataset)
            .ok_or_else(|| "The XPS dataset is no longer available.".to_owned())?;
        let xps = self.doc.datasets[index]
            .as_xps()
            .ok_or_else(|| "The selected dataset is not XPS data.".to_owned())?;
        let processed = xps.processed_region(region).ok_or_else(|| {
            "This region has no binding-energy axis, so fitting is unavailable.".to_owned()
        })?;
        let measurement = xps
            .region(region)
            .ok_or_else(|| "The XPS region is no longer available.".to_owned())?
            .measurement;
        let processing_recipe = xps
            .recipe(region)
            .cloned()
            .ok_or_else(|| "This XPS region has no processing recipe.".to_owned())?;
        let energy_shift_ev = xps
            .energy_shift(measurement)
            .ok_or_else(|| "This XPS measurement has no energy shift.".to_owned())?;
        let invocation = xps
            .fit_workspaces
            .get(&region)
            .ok_or_else(|| "This XPS region has no fitting workspace.".to_owned())?
            .invocation
            .clone();
        let input_sha256 = xps_input_sha256(
            region,
            &processed.binding_energy_ev,
            &processed.intensity,
            &invocation,
        );
        let energy = processed.binding_energy_ev;
        let intensity = processed.intensity;
        let worker_invocation = invocation.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let cancelled = || worker_cancel.load(Ordering::Relaxed);
            let result = fit_xps_peaks(&energy, &intensity, &worker_invocation, &cancelled);
            let _ = tx.send(result);
        });
        self.session.xps_fit_job = Some(XpsFitJob::Fit(XpsFitWorker {
            dataset,
            epoch: self.session.dataset_epoch,
            region,
            input_sha256,
            energy_shift_ev,
            processing_recipe,
            invocation,
            started_at: Instant::now(),
            cancel,
            rx,
        }));
        self.session.status = "Fitting XPS peaks...".into();
        Ok(())
    }

    pub fn xps_fit_progress(&self) -> Option<(DatasetId, XpsRegionId, Duration)> {
        self.session.xps_fit_job.as_ref().map(|job| {
            let (dataset, region, started_at, _) = job.common();
            (dataset, region, started_at.elapsed())
        })
    }

    pub fn cancel_xps_fit(&mut self) -> bool {
        let Some(job) = self.session.xps_fit_job.take() else {
            return false;
        };
        job.common().3.store(true, Ordering::Relaxed);
        self.session.status = "XPS analysis cancelled.".into();
        true
    }

    pub fn poll_xps_fit(&mut self) -> bool {
        let Some(job) = &self.session.xps_fit_job else {
            return false;
        };
        enum Completed {
            Fit(Result<XpsFitResult, XpsFitError>),
            Bootstrap(Result<XpsBootstrapResult, XpsFitError>),
        }
        let completed = match job {
            XpsFitJob::Fit(job) => match job.rx.try_recv() {
                Ok(result) => Completed::Fit(result),
                Err(mpsc::TryRecvError::Empty) => return true,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Completed::Fit(Err(XpsFitError::DidNotConverge))
                }
            },
            XpsFitJob::Bootstrap(job) => match job.rx.try_recv() {
                Ok(result) => Completed::Bootstrap(result),
                Err(mpsc::TryRecvError::Empty) => return true,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Completed::Bootstrap(Err(XpsFitError::DidNotConverge))
                }
            },
        };
        let job = self.session.xps_fit_job.take().expect("job checked above");
        match (job, completed) {
            (XpsFitJob::Fit(job), Completed::Fit(result)) => self.finish_xps_fit(job, result),
            (XpsFitJob::Bootstrap(job), Completed::Bootstrap(result)) => {
                self.finish_xps_bootstrap(job, result)
            }
            _ => unreachable!("job type is stable while polling"),
        }
        true
    }

    fn finish_xps_fit(&mut self, job: XpsFitWorker, result: Result<XpsFitResult, XpsFitError>) {
        match result {
            Err(error) => self.session.status = xps_fit_error(error),
            Ok(result) => {
                let current = self
                    .doc
                    .dataset_index(job.dataset)
                    .and_then(|index| (job.epoch == self.session.dataset_epoch).then_some(index))
                    .and_then(|index| {
                        let xps = self.doc.datasets[index].as_xps()?;
                        let processed = xps.processed_region(job.region)?;
                        let workspace = xps.fit_workspaces.get(&job.region)?;
                        (xps_input_sha256(
                            job.region,
                            &processed.binding_energy_ev,
                            &processed.intensity,
                            &workspace.invocation,
                        ) == job.input_sha256)
                            .then_some(())
                    })
                    .is_some();
                if !current {
                    self.session.status =
                        "The XPS input changed while fitting; the result was discarded.".into();
                    return;
                }
                let stored = StoredXpsFit {
                    region: job.region,
                    input_sha256: job.input_sha256,
                    energy_shift_ev: job.energy_shift_ev,
                    processing_recipe: job.processing_recipe,
                    invocation: job.invocation,
                    result,
                    bootstrap: None,
                };
                let r_squared = stored.result.r_squared;
                let peaks = stored.result.peaks.len();
                if let Err(error) = self.edit_xps_state(job.dataset, |state| match state {
                    DatasetProcessingState::Xps { fits, .. } => {
                        fits.entry(job.region).or_default().push(stored);
                        Ok(())
                    }
                    _ => Err("The selected dataset is not XPS data.".into()),
                }) {
                    self.session.status = error;
                } else {
                    self.session.status =
                        format!("Fitted {peaks} XPS component(s), R2 = {r_squared:.5}.");
                }
            }
        }
    }

    pub fn start_xps_bootstrap(
        &mut self,
        dataset: DatasetId,
        region: XpsRegionId,
    ) -> Result<(), String> {
        if self.session.xps_fit_job.is_some() {
            return Err("An XPS analysis is already running.".into());
        }
        let index = self
            .doc
            .dataset_index(dataset)
            .ok_or_else(|| "The XPS dataset is no longer available.".to_owned())?;
        let xps = self.doc.datasets[index]
            .as_xps()
            .ok_or_else(|| "The selected dataset is not XPS data.".to_owned())?;
        let fit = xps
            .current_fit(region)
            .ok_or_else(|| "Run the current XPS fit before Bootstrap.".to_owned())?;
        let workspace = xps
            .fit_workspaces
            .get(&region)
            .expect("current fit has workspace");
        let mut options = workspace.bootstrap.clone();
        if options.seed == 0 {
            options.seed = seed_from_hash(&fit.input_sha256);
        }
        let base = fit.result.clone();
        let invocation = fit.invocation.clone();
        let input_sha256 = fit.input_sha256.clone();
        let worker_options = options.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let cancelled = || worker_cancel.load(Ordering::Relaxed);
            let result = bootstrap_xps_fit(&base, &invocation, &worker_options, &cancelled);
            let _ = tx.send(result);
        });
        self.session.xps_fit_job = Some(XpsFitJob::Bootstrap(XpsBootstrapWorker {
            dataset,
            epoch: self.session.dataset_epoch,
            region,
            input_sha256,
            options,
            started_at: Instant::now(),
            cancel,
            rx,
        }));
        self.session.status = "Running XPS Bootstrap diagnostics...".into();
        Ok(())
    }

    fn finish_xps_bootstrap(
        &mut self,
        job: XpsBootstrapWorker,
        result: Result<XpsBootstrapResult, XpsFitError>,
    ) {
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.session.status = xps_fit_error(error);
                return;
            }
        };
        let current = self
            .doc
            .dataset_index(job.dataset)
            .and_then(|index| (job.epoch == self.session.dataset_epoch).then_some(index))
            .and_then(|index| self.doc.datasets[index].as_xps())
            .and_then(|xps| xps.current_fit(job.region))
            .is_some_and(|fit| fit.input_sha256 == job.input_sha256);
        if !current {
            self.session.status =
                "The XPS fit changed during Bootstrap; the diagnostics were discarded.".into();
            return;
        }
        let convergence = result.convergence_fraction();
        let edit = self.edit_xps_state(job.dataset, |state| match state {
            DatasetProcessingState::Xps { fits, .. } => {
                let fit = fits
                    .get_mut(&job.region)
                    .and_then(|fits| {
                        fits.iter_mut()
                            .rev()
                            .find(|fit| fit.input_sha256 == job.input_sha256)
                    })
                    .ok_or_else(|| "The fitted XPS result is no longer available.".to_owned())?;
                fit.bootstrap = Some(result);
                Ok(())
            }
            _ => Err("The selected dataset is not XPS data.".into()),
        });
        self.session.status = match edit {
            Err(error) => error,
            Ok(()) if convergence < 0.8 => format!(
                "Bootstrap completed with low convergence ({:.0}% of {} runs).",
                convergence * 100.0,
                job.options.samples
            ),
            Ok(()) => format!("Bootstrap completed ({} runs).", job.options.samples),
        };
    }
}

pub fn estimate_xps_charge_shift(
    energy: &[f64],
    intensity: &[f64],
    reference_ev: f64,
) -> Result<f64, String> {
    plotx_processing::xps::estimate_charge_shift(energy, intensity, reference_ev)
        .map_err(|message| message.to_owned())
}

pub fn xps_template(
    region_name: &str,
    intensity: &[f64],
    next_component_id: &mut u64,
) -> Option<Vec<XpsPeakSpec>> {
    let normalized = region_name.to_ascii_lowercase().replace(' ', "");
    let peaks: &[(&str, f64)] = if normalized.contains("c1s") {
        &[
            ("Aromatic C", 284.8),
            ("C=N / C-O", 286.2),
            ("O-C=O", 288.4),
        ]
    } else if normalized.contains("n1s") {
        &[("Porphyrinic N", 398.3), ("Imine N", 399.6), ("C-N", 401.0)]
    } else if normalized.contains("o1s") {
        &[("Framework O", 532.0), ("Adsorbed water", 533.2)]
    } else {
        return None;
    };
    let height = intensity
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(0.0, f64::max);
    let area = height.max(1.0);
    Some(
        peaks
            .iter()
            .map(|(label, center_ev)| {
                let id = XpsComponentId::new(*next_component_id);
                *next_component_id = next_component_id.saturating_add(1);
                XpsPeakSpec::independent(id, *label, *center_ev, area)
            })
            .collect(),
    )
}

pub fn xps_input_sha256(
    region: XpsRegionId,
    energy: &[f64],
    intensity: &[f64],
    invocation: &XpsFitInvocation,
) -> String {
    let mut digest = Sha256::new();
    digest.update(region.0.to_le_bytes());
    digest.update((energy.len() as u64).to_le_bytes());
    for value in energy.iter().chain(intensity) {
        digest.update(value.to_bits().to_le_bytes());
    }
    digest.update(
        serde_json::to_vec(invocation).expect("serializable XPS invocation has no map keys"),
    );
    format!("{:x}", digest.finalize())
}

fn seed_from_hash(hash: &str) -> u64 {
    u64::from_str_radix(hash.get(..16).unwrap_or(hash), 16).unwrap_or(1)
}

fn shift_processing_windows(recipe: &mut plotx_processing::xps::XpsProcessingRecipe, delta: f64) {
    for step in &mut recipe.steps {
        if let XpsStepKind::Window { low_ev, high_ev } = &mut step.kind {
            *low_ev += delta;
            *high_ev += delta;
        }
    }
}

fn shift_background_ranges(workspace: &mut XpsFitWorkspace, delta: f64) {
    for range in [
        &mut workspace.invocation.background.window_ev,
        &mut workspace.invocation.background.low_anchor_ev,
        &mut workspace.invocation.background.high_anchor_ev,
    ] {
        range[0] += delta;
        range[1] += delta;
    }
}

fn xps_fit_error(error: XpsFitError) -> String {
    match error {
        XpsFitError::Cancelled => "The XPS fit was cancelled.".into(),
        XpsFitError::DidNotConverge => {
            "The XPS fit did not converge. Review initial peaks and constraints.".into()
        }
        _ => error.to_string(),
    }
}

#[cfg(test)]
#[path = "app_impl_xps_tests.rs"]
mod tests;
