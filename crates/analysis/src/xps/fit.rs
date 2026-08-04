use super::*;
use crate::lineshape::{
    ConstrainedLineFitError, ConstrainedLineShapeOptions, ConstrainedPeakSpec, LineShape,
    PeakConstraintKey, PeakParameterConstraint, area_normalized_peak, fit_constrained_lineshapes,
    validate_constrained_peaks,
};

pub fn gl_peak(x: f64, center: f64, fwhm: f64, area: f64, lorentzian_fraction: f64) -> f64 {
    area_normalized_peak(
        LineShape::PseudoVoigt,
        lorentzian_fraction,
        x,
        center,
        fwhm,
        area,
    )
}

pub fn fit_xps_peaks(
    energy: &[f64],
    intensity: &[f64],
    invocation: &XpsFitInvocation,
    cancelled: &impl Fn() -> bool,
) -> Result<XpsFitResult, XpsFitError> {
    validate_options(invocation)?;
    let background = compute_xps_background(energy, intensity, &invocation.background)?;
    let specs = compile_specs(&invocation.peaks)?;
    let fit = fit_constrained_lineshapes(
        &background.energy_ev,
        &background.corrected,
        &specs,
        line_options(&invocation.options),
        cancelled,
    )
    .map_err(map_fit_error)?;

    let total_area = fit.peaks.iter().map(|peak| peak.area).sum::<f64>();
    let fractions = fit
        .peaks
        .iter()
        .map(|peak| {
            if total_area > 0.0 {
                peak.area / total_area
            } else {
                0.0
            }
        })
        .collect::<Vec<_>>();
    let physical_covariance = xps_physical_covariance(
        fit.physical_covariance.as_ref(),
        &fit.peaks.iter().map(|peak| peak.area).collect::<Vec<_>>(),
    );
    let (sigma, correlation) = covariance_diagnostics(physical_covariance.as_ref());
    let estimate = |index: usize, value: f64| parameter_estimate(value, sigma.as_ref(), index);
    let peaks = fit
        .peaks
        .iter()
        .zip(&invocation.peaks)
        .zip(&fractions)
        .enumerate()
        .map(|(index, ((result, spec), fraction))| {
            let base = index * 4;
            XpsFittedPeak {
                id: spec.id,
                label: spec.label.clone(),
                center_ev: estimate(base, result.position),
                fwhm_ev: estimate(base + 1, result.fwhm),
                area: estimate(base + 2, result.area),
                fraction: estimate(base + 3, *fraction),
                hit_position_bound: result.hit_position_bound,
                hit_fwhm_bound: result.hit_fwhm_bound,
                hit_area_bound: result.hit_area_bound,
            }
        })
        .collect::<Vec<_>>();
    let envelope = background
        .background
        .iter()
        .zip(&fit.total)
        .map(|(background, peaks)| background + peaks)
        .collect::<Vec<_>>();
    let ss_res = fit.residual.iter().map(|value| value * value).sum::<f64>();
    let mean = background.intensity.iter().sum::<f64>() / background.intensity.len() as f64;
    let ss_tot = background
        .intensity
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>();
    Ok(XpsFitResult {
        energy_ev: background.energy_ev,
        intensity: background.intensity,
        background: background.background,
        envelope,
        residual: fit.residual.clone(),
        components: fit.components,
        peaks,
        parameter_labels: invocation
            .peaks
            .iter()
            .flat_map(|peak| {
                ["center", "FWHM", "area", "fraction"]
                    .map(|parameter| format!("{} {parameter}", peak.label))
            })
            .collect(),
        parameter_correlation: correlation,
        r_squared: if ss_tot > f64::MIN_POSITIVE {
            1.0 - ss_res / ss_tot
        } else {
            0.0
        },
        rmse: (ss_res / fit.residual.len() as f64).sqrt(),
        residual_lag1: residual_lag1(&fit.residual),
    })
}

/// Rebuild curve arrays for a persisted PlotX result without running the solver.
pub fn rebuild_xps_fit_curves(
    energy: &[f64],
    intensity: &[f64],
    invocation: &XpsFitInvocation,
    result: &mut XpsFitResult,
) -> Result<(), XpsFitError> {
    validate_xps_fit_summary(invocation, result)?;
    let background = compute_xps_background(energy, intensity, &invocation.background)?;
    let fitted = invocation
        .peaks
        .iter()
        .map(|spec| {
            result
                .peaks
                .iter()
                .find(|peak| peak.id == spec.id)
                .ok_or_else(|| {
                    XpsFitError::InvalidConstraints(
                        "persisted result does not match its component IDs".into(),
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let components = fitted
        .iter()
        .map(|peak| {
            background
                .energy_ev
                .iter()
                .map(|&x| {
                    gl_peak(
                        x,
                        peak.center_ev.value,
                        peak.fwhm_ev.value,
                        peak.area.value,
                        invocation.options.lorentzian_fraction,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let envelope = (0..background.energy_ev.len())
        .map(|index| {
            background.background[index]
                + components
                    .iter()
                    .map(|component| component[index])
                    .sum::<f64>()
        })
        .collect::<Vec<_>>();
    let residual = background
        .intensity
        .iter()
        .zip(&envelope)
        .map(|(observed, predicted)| observed - predicted)
        .collect();
    result.energy_ev = background.energy_ev;
    result.intensity = background.intensity;
    result.background = background.background;
    result.envelope = envelope;
    result.residual = residual;
    result.components = components;
    Ok(())
}

pub fn validate_xps_fit_summary(
    invocation: &XpsFitInvocation,
    result: &XpsFitResult,
) -> Result<(), XpsFitError> {
    validate_xps_constraints(invocation)?;
    if result.peaks.len() != invocation.peaks.len()
        || result.parameter_labels.len() != result.peaks.len() * 4
        || !result.r_squared.is_finite()
        || !result.rmse.is_finite()
        || result.rmse < 0.0
        || result.residual_lag1.is_some_and(|value| !value.is_finite())
    {
        return Err(XpsFitError::InvalidInput);
    }
    for (spec, peak) in invocation.peaks.iter().zip(&result.peaks) {
        if spec.id != peak.id
            || !valid_estimate(&peak.center_ev)
            || !valid_estimate(&peak.fwhm_ev)
            || !valid_estimate(&peak.area)
            || !valid_estimate(&peak.fraction)
            || peak.fwhm_ev.value <= 0.0
            || peak.area.value < 0.0
            || !(0.0..=1.0 + 1e-9).contains(&peak.fraction.value)
        {
            return Err(XpsFitError::InvalidInput);
        }
    }
    if result.parameter_correlation.as_ref().is_some_and(|matrix| {
        let n = result.parameter_labels.len();
        matrix.len() != n
            || matrix.iter().any(|row| {
                row.len() != n
                    || row
                        .iter()
                        .any(|value| !value.is_finite() || value.abs() > 1.0 + 1e-6)
            })
    }) {
        return Err(XpsFitError::InvalidInput);
    }
    Ok(())
}

fn valid_estimate(value: &XpsParameterEstimate) -> bool {
    value.value.is_finite()
        && value
            .standard_error
            .is_none_or(|error| error.is_finite() && error >= 0.0)
        && value
            .confidence_95
            .is_none_or(|bounds| bounds.iter().all(|bound| bound.is_finite()))
}

pub fn validate_xps_constraints(invocation: &XpsFitInvocation) -> Result<(), XpsFitError> {
    validate_options(invocation)?;
    if invocation.peaks.is_empty() {
        return Ok(());
    }
    let specs = compile_specs(&invocation.peaks)?;
    validate_constrained_peaks(&specs).map_err(map_fit_error)
}

fn compile_specs(peaks: &[XpsPeakSpec]) -> Result<Vec<ConstrainedPeakSpec>, XpsFitError> {
    peaks
        .iter()
        .map(|peak| {
            Ok(ConstrainedPeakSpec {
                key: key(peak.id),
                position: match peak.center {
                    XpsCenterConstraint::Free {
                        initial_ev,
                        bounds_ev,
                    } => free(initial_ev, bounds_ev),
                    XpsCenterConstraint::Fixed { value_ev } => fixed(value_ev),
                    XpsCenterConstraint::Offset {
                        reference,
                        delta_ev,
                    } => linked(reference, 1.0, delta_ev),
                },
                fwhm: match peak.fwhm {
                    XpsFwhmConstraint::Free {
                        initial_ev,
                        bounds_ev,
                    } => free(initial_ev, bounds_ev),
                    XpsFwhmConstraint::Fixed { value_ev } => fixed(value_ev),
                    XpsFwhmConstraint::Shared { reference } => linked(reference, 1.0, 0.0),
                },
                area: match peak.area {
                    XpsAreaConstraint::Free { initial, bounds } => free(initial, bounds),
                    XpsAreaConstraint::Fixed { value } => fixed(value),
                    XpsAreaConstraint::Ratio { reference, ratio } => linked(reference, ratio, 0.0),
                },
            })
        })
        .collect()
}

fn free(initial: f64, bounds: [f64; 2]) -> PeakParameterConstraint {
    PeakParameterConstraint::Free { initial, bounds }
}

fn fixed(value: f64) -> PeakParameterConstraint {
    PeakParameterConstraint::Fixed { value }
}

fn linked(reference: XpsComponentId, scale: f64, offset: f64) -> PeakParameterConstraint {
    PeakParameterConstraint::Linked {
        reference: key(reference),
        scale,
        offset,
    }
}

fn key(id: XpsComponentId) -> PeakConstraintKey {
    PeakConstraintKey(id.0)
}

fn line_options(options: &XpsFitOptions) -> ConstrainedLineShapeOptions {
    ConstrainedLineShapeOptions {
        shape: LineShape::PseudoVoigt,
        pseudo_voigt_fraction: options.lorentzian_fraction,
        max_iterations: options.max_iterations,
    }
}

fn validate_options(invocation: &XpsFitInvocation) -> Result<(), XpsFitError> {
    if invocation.options.max_iterations == 0
        || !(0.0..=1.0).contains(&invocation.options.lorentzian_fraction)
    {
        return Err(XpsFitError::InvalidInput);
    }
    Ok(())
}

fn map_fit_error(error: ConstrainedLineFitError) -> XpsFitError {
    match error {
        ConstrainedLineFitError::InvalidInput => XpsFitError::InvalidInput,
        ConstrainedLineFitError::InvalidConstraints(message) => {
            XpsFitError::InvalidConstraints(message)
        }
        ConstrainedLineFitError::Cancelled => XpsFitError::Cancelled,
        ConstrainedLineFitError::DidNotConverge => XpsFitError::DidNotConverge,
    }
}

fn xps_physical_covariance(
    covariance: Option<&Vec<Vec<f64>>>,
    areas: &[f64],
) -> Option<Vec<Vec<f64>>> {
    let covariance = covariance?;
    let source_count = areas.len() * 3;
    if covariance.len() != source_count || covariance.iter().any(|row| row.len() != source_count) {
        return None;
    }
    let target_count = areas.len() * 4;
    let total = areas.iter().sum::<f64>();
    let mut transform = vec![vec![0.0; source_count]; target_count];
    for peak in 0..areas.len() {
        transform[peak * 4][peak * 3] = 1.0;
        transform[peak * 4 + 1][peak * 3 + 1] = 1.0;
        transform[peak * 4 + 2][peak * 3 + 2] = 1.0;
        if total > f64::MIN_POSITIVE {
            for area in 0..areas.len() {
                let numerator = if area == peak {
                    total - areas[peak]
                } else {
                    -areas[peak]
                };
                transform[peak * 4 + 3][area * 3 + 2] = numerator / (total * total);
            }
        }
    }
    let mut output = vec![vec![0.0; target_count]; target_count];
    for a in 0..target_count {
        for b in 0..target_count {
            for i in 0..source_count {
                for j in 0..source_count {
                    output[a][b] += transform[a][i] * covariance[i][j] * transform[b][j];
                }
            }
        }
    }
    Some(output)
}

fn covariance_diagnostics(
    covariance: Option<&Vec<Vec<f64>>>,
) -> (Option<Vec<f64>>, Option<Vec<Vec<f64>>>) {
    let covariance = match covariance {
        Some(covariance) => covariance,
        None => return (None, None),
    };
    let sigma = (0..covariance.len())
        .map(|index| covariance[index][index].max(0.0).sqrt())
        .collect::<Vec<_>>();
    let correlation = (0..covariance.len())
        .map(|row| {
            (0..covariance.len())
                .map(|column| {
                    let scale = sigma[row] * sigma[column];
                    if scale > f64::MIN_POSITIVE {
                        covariance[row][column] / scale
                    } else if row == column {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect()
        })
        .collect();
    (Some(sigma), Some(correlation))
}

fn parameter_estimate(value: f64, sigma: Option<&Vec<f64>>, index: usize) -> XpsParameterEstimate {
    let standard_error = sigma.and_then(|values| values.get(index).copied());
    XpsParameterEstimate {
        value,
        standard_error,
        confidence_95: standard_error.map(|sigma| [value - 1.96 * sigma, value + 1.96 * sigma]),
    }
}

fn residual_lag1(values: &[f64]) -> Option<f64> {
    if values.len() < 3 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let denominator = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>();
    if denominator <= f64::MIN_POSITIVE {
        return None;
    }
    Some(
        values
            .windows(2)
            .map(|pair| (pair[0] - mean) * (pair[1] - mean))
            .sum::<f64>()
            / denominator,
    )
}
