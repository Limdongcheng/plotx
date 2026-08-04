use std::collections::BTreeMap;

pub(super) fn add_statistics(out: &mut BTreeMap<String, f64>, values: &[f64]) {
    let finite = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if finite.is_empty() {
        return;
    }
    out.insert(
        "min".to_owned(),
        finite.iter().copied().fold(f64::INFINITY, f64::min),
    );
    out.insert(
        "max".to_owned(),
        finite.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    out.insert(
        "mean".to_owned(),
        finite.iter().sum::<f64>() / finite.len() as f64,
    );
}
