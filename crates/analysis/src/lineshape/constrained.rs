use super::{LineShape, peak_partials};
use crate::fit::{
    LmProblem, levenberg_marquardt_problem_cancellable, mirror_upper, problem_parameter_covariance,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeakConstraintKey(pub u64);

#[derive(Debug, Clone, PartialEq)]
pub enum PeakParameterConstraint {
    Free {
        initial: f64,
        bounds: [f64; 2],
    },
    Fixed {
        value: f64,
    },
    Linked {
        reference: PeakConstraintKey,
        scale: f64,
        offset: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstrainedPeakSpec {
    pub key: PeakConstraintKey,
    pub position: PeakParameterConstraint,
    pub fwhm: PeakParameterConstraint,
    pub area: PeakParameterConstraint,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConstrainedLineShapeOptions {
    pub shape: LineShape,
    pub pseudo_voigt_fraction: f64,
    pub max_iterations: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstrainedPeakResult {
    pub key: PeakConstraintKey,
    pub position: f64,
    pub fwhm: f64,
    pub area: f64,
    pub hit_position_bound: bool,
    pub hit_fwhm_bound: bool,
    pub hit_area_bound: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstrainedLineFit {
    pub peaks: Vec<ConstrainedPeakResult>,
    pub components: Vec<Vec<f64>>,
    pub total: Vec<f64>,
    pub residual: Vec<f64>,
    /// Covariance of `[position, FWHM, area]` blocks in input component order.
    pub physical_covariance: Option<Vec<Vec<f64>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConstrainedLineFitError {
    #[error("line-fit arrays or options are invalid")]
    InvalidInput,
    #[error("line-fit constraints are invalid: {0}")]
    InvalidConstraints(String),
    #[error("line fit was cancelled")]
    Cancelled,
    #[error("line fit did not converge")]
    DidNotConverge,
}

#[derive(Clone)]
struct FreeParameter {
    initial: f64,
    bounds: [f64; 2],
}

struct CompiledConstraints<'a> {
    peaks: &'a [ConstrainedPeakSpec],
    key_index: BTreeMap<PeakConstraintKey, usize>,
    orders: [Vec<usize>; 3],
    free_index: [Vec<Option<usize>>; 3],
    free: Vec<FreeParameter>,
}

impl<'a> CompiledConstraints<'a> {
    fn new(peaks: &'a [ConstrainedPeakSpec]) -> Result<Self, ConstrainedLineFitError> {
        if peaks.is_empty() {
            return Err(invalid("add at least one component"));
        }
        let mut key_index = BTreeMap::new();
        for (index, peak) in peaks.iter().enumerate() {
            if key_index.insert(peak.key, index).is_some() {
                return Err(invalid("component keys must be unique"));
            }
        }
        let orders = [
            dependency_order(peaks, &key_index, |peak| &peak.position)?,
            dependency_order(peaks, &key_index, |peak| &peak.fwhm)?,
            dependency_order(peaks, &key_index, |peak| &peak.area)?,
        ];
        let mut free = Vec::new();
        let mut free_index = std::array::from_fn(|_| vec![None; peaks.len()]);
        for (peak_index, peak) in peaks.iter().enumerate() {
            for (kind, parameter) in [&peak.position, &peak.fwhm, &peak.area]
                .into_iter()
                .enumerate()
            {
                match parameter {
                    PeakParameterConstraint::Free { initial, bounds } => {
                        free_index[kind][peak_index] =
                            Some(push_free(&mut free, *initial, *bounds)?);
                    }
                    PeakParameterConstraint::Fixed { value } if value.is_finite() => {}
                    PeakParameterConstraint::Linked { scale, offset, .. }
                        if scale.is_finite() && offset.is_finite() => {}
                    _ => return Err(invalid("parameter values must be finite")),
                }
            }
        }
        Ok(Self {
            peaks,
            key_index,
            orders,
            free_index,
            free,
        })
    }

    fn initial(&self) -> Vec<f64> {
        self.free
            .iter()
            .map(|parameter| from_bound(parameter.initial, parameter.bounds))
            .collect()
    }

    fn decode_with_jacobian(&self, free: &[f64]) -> (Vec<[f64; 3]>, Vec<Vec<Vec<f64>>>) {
        let mut values = vec![[0.0; 3]; self.peaks.len()];
        let mut jacobian = vec![vec![vec![0.0; free.len()]; 3]; self.peaks.len()];
        for kind in 0..3 {
            for &peak_index in &self.orders[kind] {
                let parameter = parameter(&self.peaks[peak_index], kind);
                match parameter {
                    PeakParameterConstraint::Free { bounds, .. } => {
                        let free_index = self.free_index[kind][peak_index]
                            .expect("compiled free parameter has an index");
                        values[peak_index][kind] = to_bound(free[free_index], *bounds);
                        jacobian[peak_index][kind][free_index] =
                            bound_derivative(free[free_index], *bounds);
                    }
                    PeakParameterConstraint::Fixed { value } => {
                        values[peak_index][kind] = *value;
                    }
                    PeakParameterConstraint::Linked {
                        reference,
                        scale,
                        offset,
                    } => {
                        let reference = self.key_index[reference];
                        values[peak_index][kind] = values[reference][kind] * scale + offset;
                        let reference_derivatives = jacobian[reference][kind].clone();
                        for (derivative, reference_derivative) in jacobian[peak_index][kind]
                            .iter_mut()
                            .zip(reference_derivatives)
                        {
                            *derivative = reference_derivative * scale;
                        }
                    }
                }
            }
        }
        (values, jacobian)
    }
}

struct ConstrainedProblem<'a> {
    x: &'a [f64],
    y: &'a [f64],
    constraints: &'a CompiledConstraints<'a>,
    options: ConstrainedLineShapeOptions,
}

impl LmProblem for ConstrainedProblem<'_> {
    fn cost(&mut self, free: &[f64]) -> f64 {
        let (values, _) = self.constraints.decode_with_jacobian(free);
        self.x
            .iter()
            .zip(self.y)
            .map(|(&x, &y)| {
                let residual = y - component_sum(x, &values, self.options);
                residual * residual
            })
            .sum()
    }

    fn normal_equations(&mut self, free: &[f64], jtj: &mut [Vec<f64>], jtr: &mut [f64]) {
        for row in jtj.iter_mut() {
            row.fill(0.0);
        }
        jtr.fill(0.0);
        let (values, physical_jacobian) = self.constraints.decode_with_jacobian(free);
        let mut gradient = vec![0.0; free.len()];
        for (&x, &y) in self.x.iter().zip(self.y) {
            gradient.fill(0.0);
            let mut predicted = 0.0;
            for (peak, jacobian) in values.iter().zip(&physical_jacobian) {
                let (value, derivatives) = component_value_and_partials(x, *peak, self.options);
                predicted += value;
                for free_index in 0..free.len() {
                    for kind in 0..3 {
                        gradient[free_index] += derivatives[kind] * jacobian[kind][free_index];
                    }
                }
            }
            let residual = y - predicted;
            for a in 0..free.len() {
                jtr[a] += gradient[a] * residual;
                for b in a..free.len() {
                    jtj[a][b] += gradient[a] * gradient[b];
                }
            }
        }
        mirror_upper(jtj);
    }
}

pub fn validate_constrained_peaks(
    peaks: &[ConstrainedPeakSpec],
) -> Result<(), ConstrainedLineFitError> {
    if peaks.is_empty() {
        return Ok(());
    }
    let constraints = CompiledConstraints::new(peaks)?;
    let initial = constraints.initial();
    let (values, _) = constraints.decode_with_jacobian(&initial);
    validate_physical(&values)?;
    Ok(())
}

pub fn fit_constrained_lineshapes(
    x: &[f64],
    y: &[f64],
    peaks: &[ConstrainedPeakSpec],
    options: ConstrainedLineShapeOptions,
    cancelled: &impl Fn() -> bool,
) -> Result<ConstrainedLineFit, ConstrainedLineFitError> {
    if x.len() != y.len()
        || x.len() < 3
        || x.iter().chain(y).any(|value| !value.is_finite())
        || options.max_iterations == 0
        || !(0.0..=1.0).contains(&options.pseudo_voigt_fraction)
    {
        return Err(ConstrainedLineFitError::InvalidInput);
    }
    if cancelled() {
        return Err(ConstrainedLineFitError::Cancelled);
    }
    let constraints = CompiledConstraints::new(peaks)?;
    let initial = constraints.initial();
    let (initial_values, _) = constraints.decode_with_jacobian(&initial);
    validate_physical(&initial_values)?;
    let mut problem = ConstrainedProblem {
        x,
        y,
        constraints: &constraints,
        options,
    };
    let free = if initial.is_empty() {
        Vec::new()
    } else {
        levenberg_marquardt_problem_cancellable(
            &mut problem,
            &initial,
            options.max_iterations,
            cancelled,
        )
        .map(|(parameters, _)| parameters)
        .ok_or_else(|| {
            if cancelled() {
                ConstrainedLineFitError::Cancelled
            } else {
                ConstrainedLineFitError::DidNotConverge
            }
        })?
    };
    let free_covariance = problem_parameter_covariance(&mut problem, &free, x.len());
    let (values, physical_jacobian) = constraints.decode_with_jacobian(&free);
    validate_physical(&values)?;
    let physical_covariance = propagate_covariance(&physical_jacobian, free_covariance.as_ref());
    let components = values
        .iter()
        .map(|&peak| {
            x.iter()
                .map(|&x| component_value_and_partials(x, peak, options).0)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let total = (0..x.len())
        .map(|row| components.iter().map(|component| component[row]).sum())
        .collect::<Vec<_>>();
    let residual = y.iter().zip(&total).map(|(y, fit)| y - fit).collect();
    Ok(ConstrainedLineFit {
        peaks: peaks
            .iter()
            .zip(&values)
            .map(|(spec, values)| ConstrainedPeakResult {
                key: spec.key,
                position: values[0],
                fwhm: values[1],
                area: values[2],
                hit_position_bound: hit_bound(&spec.position, values[0]),
                hit_fwhm_bound: hit_bound(&spec.fwhm, values[1]),
                hit_area_bound: hit_bound(&spec.area, values[2]),
            })
            .collect(),
        components,
        total,
        residual,
        physical_covariance,
    })
}

pub fn area_normalized_peak(
    shape: LineShape,
    pseudo_voigt_fraction: f64,
    x: f64,
    position: f64,
    fwhm: f64,
    area: f64,
) -> f64 {
    component_value_and_partials(
        x,
        [position, fwhm, area],
        ConstrainedLineShapeOptions {
            shape,
            pseudo_voigt_fraction,
            max_iterations: 1,
        },
    )
    .0
}

fn component_value_and_partials(
    x: f64,
    peak: [f64; 3],
    options: ConstrainedLineShapeOptions,
) -> (f64, [f64; 3]) {
    let [position, fwhm, area] = peak;
    if fwhm <= 0.0 || area < 0.0 {
        return (0.0, [0.0; 3]);
    }
    let eta = options.pseudo_voigt_fraction;
    let factor = options.shape.area_factor(eta);
    let height = area / (fwhm * factor);
    let partials = peak_partials(options.shape, x - position, height, fwhm, eta);
    let value = height * options.shape.unit(x - position, fwhm, eta);
    (
        value,
        [
            partials[0],
            partials[2] - partials[1] * height / fwhm,
            partials[1] / (fwhm * factor),
        ],
    )
}

fn component_sum(x: f64, peaks: &[[f64; 3]], options: ConstrainedLineShapeOptions) -> f64 {
    peaks
        .iter()
        .map(|&peak| component_value_and_partials(x, peak, options).0)
        .sum()
}

fn parameter(peak: &ConstrainedPeakSpec, kind: usize) -> &PeakParameterConstraint {
    match kind {
        0 => &peak.position,
        1 => &peak.fwhm,
        _ => &peak.area,
    }
}

fn dependency_order(
    peaks: &[ConstrainedPeakSpec],
    keys: &BTreeMap<PeakConstraintKey, usize>,
    parameter: impl Fn(&ConstrainedPeakSpec) -> &PeakParameterConstraint,
) -> Result<Vec<usize>, ConstrainedLineFitError> {
    fn visit(
        index: usize,
        peaks: &[ConstrainedPeakSpec],
        keys: &BTreeMap<PeakConstraintKey, usize>,
        parameter: &impl Fn(&ConstrainedPeakSpec) -> &PeakParameterConstraint,
        visiting: &mut BTreeSet<usize>,
        visited: &mut BTreeSet<usize>,
        out: &mut Vec<usize>,
    ) -> Result<(), ConstrainedLineFitError> {
        if visited.contains(&index) {
            return Ok(());
        }
        if !visiting.insert(index) {
            return Err(invalid("parameter links contain a cycle"));
        }
        if let PeakParameterConstraint::Linked { reference, .. } = parameter(&peaks[index]) {
            let target = *keys
                .get(reference)
                .ok_or_else(|| invalid("a parameter link targets a missing component"))?;
            if target == index {
                return Err(invalid("a component cannot link to itself"));
            }
            visit(target, peaks, keys, parameter, visiting, visited, out)?;
        }
        visiting.remove(&index);
        visited.insert(index);
        out.push(index);
        Ok(())
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut out = Vec::with_capacity(peaks.len());
    for index in 0..peaks.len() {
        visit(
            index,
            peaks,
            keys,
            &parameter,
            &mut visiting,
            &mut visited,
            &mut out,
        )?;
    }
    Ok(out)
}

fn push_free(
    free: &mut Vec<FreeParameter>,
    initial: f64,
    bounds: [f64; 2],
) -> Result<usize, ConstrainedLineFitError> {
    if !initial.is_finite()
        || bounds.iter().any(|value| !value.is_finite())
        || bounds[0] >= bounds[1]
        || initial < bounds[0]
        || initial > bounds[1]
    {
        return Err(invalid(
            "free parameter bounds must be ordered and contain the initial value",
        ));
    }
    let index = free.len();
    free.push(FreeParameter { initial, bounds });
    Ok(index)
}

fn validate_physical(values: &[[f64; 3]]) -> Result<(), ConstrainedLineFitError> {
    if values
        .iter()
        .any(|peak| peak.iter().any(|value| !value.is_finite()) || peak[1] <= 0.0 || peak[2] < 0.0)
    {
        return Err(invalid(
            "linked FWHM must stay positive and linked area non-negative",
        ));
    }
    Ok(())
}

fn propagate_covariance(
    jacobian: &[Vec<Vec<f64>>],
    covariance: Option<&Vec<Vec<f64>>>,
) -> Option<Vec<Vec<f64>>> {
    let covariance = covariance?;
    let rows = jacobian.len() * 3;
    let free = covariance.len();
    let mut output = vec![vec![0.0; rows]; rows];
    for a in 0..rows {
        for b in 0..rows {
            for i in 0..free {
                for j in 0..free {
                    output[a][b] +=
                        jacobian[a / 3][a % 3][i] * covariance[i][j] * jacobian[b / 3][b % 3][j];
                }
            }
        }
    }
    Some(output)
}

fn hit_bound(spec: &PeakParameterConstraint, value: f64) -> bool {
    match spec {
        PeakParameterConstraint::Free { bounds, .. } => {
            (value - bounds[0]).abs() < 1e-3 || (value - bounds[1]).abs() < 1e-3
        }
        _ => false,
    }
}

fn to_bound(value: f64, bounds: [f64; 2]) -> f64 {
    bounds[0] + (bounds[1] - bounds[0]) / (1.0 + (-value).exp())
}

fn from_bound(value: f64, bounds: [f64; 2]) -> f64 {
    let ratio = ((value - bounds[0]) / (bounds[1] - bounds[0])).clamp(1e-12, 1.0 - 1e-12);
    (ratio / (1.0 - ratio)).ln()
}

fn bound_derivative(value: f64, bounds: [f64; 2]) -> f64 {
    let logistic = 1.0 / (1.0 + (-value).exp());
    (bounds[1] - bounds[0]) * logistic * (1.0 - logistic)
}

fn invalid(message: impl Into<String>) -> ConstrainedLineFitError {
    ConstrainedLineFitError::InvalidConstraints(message.into())
}

#[cfg(test)]
#[path = "constrained_tests.rs"]
mod tests;
