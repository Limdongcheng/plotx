use super::{AutomationError, ResourceRef, add_statistics};
use crate::state::XpsDataset;
use std::collections::BTreeMap;

pub(super) fn descriptor(dataset: &XpsDataset) -> (Vec<usize>, Vec<String>, Vec<ResourceRef>) {
    (
        vec![
            dataset.experiment.measurements.len(),
            dataset.experiment.regions.len(),
        ],
        vec!["eV".to_owned()],
        Vec::new(),
    )
}

pub(super) fn preview(
    dataset: &XpsDataset,
    target: &ResourceRef,
    limit: usize,
    statistics: &mut BTreeMap<String, f64>,
) -> Result<(Vec<usize>, serde_json::Value, usize), AutomationError> {
    let region = target
        .local_id
        .as_deref()
        .and_then(|key| dataset.field_catalog.id_for_key(key))
        .and_then(|id| dataset.region_for_field(id))
        .unwrap_or_else(|| dataset.active_region());
    let processed = dataset.displayed_region(region.id).ok_or_else(|| {
        AutomationError::Execution("XPS region has no displayable energy axis".into())
    })?;
    add_statistics(statistics, &processed.intensity);
    let rows = processed
        .binding_energy_ev
        .iter()
        .zip(&processed.intensity)
        .take(limit)
        .map(|(x, y)| serde_json::json!([x, y]))
        .collect::<Vec<_>>();
    Ok((
        vec![processed.intensity.len(), 2],
        serde_json::Value::Array(rows),
        processed.intensity.len(),
    ))
}
