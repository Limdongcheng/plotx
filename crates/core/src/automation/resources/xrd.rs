use super::add_statistics;
use crate::state::XrdDataset;
use std::collections::BTreeMap;

pub(super) fn preview(
    dataset: &XrdDataset,
    limit: usize,
    statistics: &mut BTreeMap<String, f64>,
) -> (Vec<usize>, serde_json::Value, usize) {
    add_statistics(statistics, &dataset.processed.intensity);
    let values = dataset
        .processed
        .intensity
        .iter()
        .take(limit)
        .map(|value| {
            if value.is_finite() {
                serde_json::json!(value)
            } else {
                serde_json::Value::Null
            }
        })
        .collect::<Vec<_>>();
    (
        vec![dataset.data.len()],
        serde_json::Value::Array(values),
        dataset.data.len(),
    )
}
