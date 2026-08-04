//! XPS-specific background and constrained peak analysis.

mod bootstrap;
mod fit;

pub use bootstrap::*;
pub use fit::*;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct XpsComponentId(pub u64);

impl XpsComponentId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum XpsBackgroundModel {
    Linear,
    Shirley {
        tolerance: f64,
        max_iterations: usize,
    },
    TougaardU2 {
        b_ev2: f64,
        c_ev2: f64,
    },
}

impl Default for XpsBackgroundModel {
    fn default() -> Self {
        Self::Shirley {
            tolerance: 1e-6,
            max_iterations: 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XpsBackgroundSpec {
    pub model: XpsBackgroundModel,
    pub window_ev: [f64; 2],
    pub low_anchor_ev: [f64; 2],
    pub high_anchor_ev: [f64; 2],
}

impl XpsBackgroundSpec {
    pub fn suggested(energy: &[f64]) -> Option<Self> {
        if energy.len() < 3 || energy.iter().any(|value| !value.is_finite()) {
            return None;
        }
        let low = energy.iter().copied().reduce(f64::min)?;
        let high = energy.iter().copied().reduce(f64::max)?;
        if high <= low {
            return None;
        }
        let mut sorted = energy.to_vec();
        sorted.sort_by(f64::total_cmp);
        let edge = sorted.len().min(3);
        Some(Self {
            model: XpsBackgroundModel::default(),
            window_ev: [low, high],
            low_anchor_ev: [sorted[0], sorted[edge - 1]],
            high_anchor_ev: [sorted[sorted.len() - edge], sorted[sorted.len() - 1]],
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XpsBackgroundResult {
    pub energy_ev: Vec<f64>,
    pub intensity: Vec<f64>,
    pub background: Vec<f64>,
    pub corrected: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum XpsCenterConstraint {
    Free {
        initial_ev: f64,
        bounds_ev: [f64; 2],
    },
    Fixed {
        value_ev: f64,
    },
    Offset {
        reference: XpsComponentId,
        delta_ev: f64,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum XpsFwhmConstraint {
    Free {
        initial_ev: f64,
        bounds_ev: [f64; 2],
    },
    Fixed {
        value_ev: f64,
    },
    Shared {
        reference: XpsComponentId,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum XpsAreaConstraint {
    Free {
        initial: f64,
        bounds: [f64; 2],
    },
    Fixed {
        value: f64,
    },
    Ratio {
        reference: XpsComponentId,
        ratio: f64,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XpsPeakSpec {
    pub id: XpsComponentId,
    pub label: String,
    pub center: XpsCenterConstraint,
    pub fwhm: XpsFwhmConstraint,
    pub area: XpsAreaConstraint,
}

impl XpsPeakSpec {
    pub fn independent(
        id: XpsComponentId,
        label: impl Into<String>,
        center_ev: f64,
        area: f64,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            center: XpsCenterConstraint::Free {
                initial_ev: center_ev,
                bounds_ev: [center_ev - 0.8, center_ev + 0.8],
            },
            fwhm: XpsFwhmConstraint::Free {
                initial_ev: 1.2,
                bounds_ev: [0.8, 2.5],
            },
            area: XpsAreaConstraint::Free {
                initial: area.max(f64::MIN_POSITIVE),
                bounds: [0.0, (area.abs() * 20.0).max(1.0)],
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XpsFitOptions {
    pub lorentzian_fraction: f64,
    pub max_iterations: usize,
}

impl Default for XpsFitOptions {
    fn default() -> Self {
        Self {
            lorentzian_fraction: 0.3,
            max_iterations: 5_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XpsFitInvocation {
    pub background: XpsBackgroundSpec,
    pub peaks: Vec<XpsPeakSpec>,
    pub options: XpsFitOptions,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XpsParameterEstimate {
    pub value: f64,
    pub standard_error: Option<f64>,
    pub confidence_95: Option<[f64; 2]>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XpsFittedPeak {
    pub id: XpsComponentId,
    pub label: String,
    pub center_ev: XpsParameterEstimate,
    pub fwhm_ev: XpsParameterEstimate,
    pub area: XpsParameterEstimate,
    pub fraction: XpsParameterEstimate,
    pub hit_position_bound: bool,
    pub hit_fwhm_bound: bool,
    pub hit_area_bound: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XpsFitResult {
    #[serde(skip)]
    pub energy_ev: Vec<f64>,
    #[serde(skip)]
    pub intensity: Vec<f64>,
    #[serde(skip)]
    pub background: Vec<f64>,
    #[serde(skip)]
    pub envelope: Vec<f64>,
    #[serde(skip)]
    pub residual: Vec<f64>,
    #[serde(skip)]
    pub components: Vec<Vec<f64>>,
    pub peaks: Vec<XpsFittedPeak>,
    pub parameter_labels: Vec<String>,
    pub parameter_correlation: Option<Vec<Vec<f64>>>,
    pub r_squared: f64,
    pub rmse: f64,
    pub residual_lag1: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum XpsFitError {
    #[error("XPS arrays are invalid")]
    InvalidInput,
    #[error("XPS background specification is invalid")]
    InvalidBackground,
    #[error("XPS peak constraints are invalid: {0}")]
    InvalidConstraints(String),
    #[error("XPS fit was cancelled")]
    Cancelled,
    #[error("XPS fit did not converge")]
    DidNotConverge,
}

pub fn compute_xps_background(
    energy: &[f64],
    intensity: &[f64],
    spec: &XpsBackgroundSpec,
) -> Result<XpsBackgroundResult, XpsFitError> {
    let (x, y) = selected_window(energy, intensity, spec.window_ev)?;
    let low = anchor_mean(&x, &y, spec.low_anchor_ev)?;
    let high = anchor_mean(&x, &y, spec.high_anchor_ev)?;
    let background = match spec.model {
        XpsBackgroundModel::Linear => linear_background(&x, low, high),
        XpsBackgroundModel::Shirley {
            tolerance,
            max_iterations,
        } => shirley_with_levels(&x, &y, low, high, tolerance, max_iterations)?,
        XpsBackgroundModel::TougaardU2 { b_ev2, c_ev2 } => {
            tougaard_u2_background(&x, &y, low, high, b_ev2, c_ev2)?
        }
    };
    let corrected = y
        .iter()
        .zip(&background)
        .map(|(value, base)| value - base)
        .collect();
    Ok(XpsBackgroundResult {
        energy_ev: x,
        intensity: y,
        background,
        corrected,
    })
}

pub fn shirley_background(
    energy: &[f64],
    intensity: &[f64],
    tolerance: f64,
    max_iterations: usize,
) -> Result<Vec<f64>, XpsFitError> {
    let spec = XpsBackgroundSpec::suggested(energy).ok_or(XpsFitError::InvalidInput)?;
    let mut spec = spec;
    spec.model = XpsBackgroundModel::Shirley {
        tolerance,
        max_iterations,
    };
    Ok(compute_xps_background(energy, intensity, &spec)?.background)
}

fn selected_window(
    energy: &[f64],
    intensity: &[f64],
    bounds: [f64; 2],
) -> Result<(Vec<f64>, Vec<f64>), XpsFitError> {
    if energy.len() != intensity.len()
        || energy.len() < 3
        || energy
            .iter()
            .chain(intensity)
            .any(|value| !value.is_finite())
        || bounds.iter().any(|value| !value.is_finite())
    {
        return Err(XpsFitError::InvalidInput);
    }
    let low = bounds[0].min(bounds[1]);
    let high = bounds[0].max(bounds[1]);
    let (x, y): (Vec<_>, Vec<_>) = energy
        .iter()
        .copied()
        .zip(intensity.iter().copied())
        .filter(|(value, _)| *value >= low && *value <= high)
        .unzip();
    if x.len() < 8 {
        return Err(XpsFitError::InvalidBackground);
    }
    Ok((x, y))
}

fn anchor_mean(x: &[f64], y: &[f64], bounds: [f64; 2]) -> Result<f64, XpsFitError> {
    if bounds.iter().any(|value| !value.is_finite()) {
        return Err(XpsFitError::InvalidBackground);
    }
    let low = bounds[0].min(bounds[1]);
    let high = bounds[0].max(bounds[1]);
    let values = x
        .iter()
        .zip(y)
        .filter_map(|(&x, &y)| (x >= low && x <= high).then_some(y))
        .collect::<Vec<_>>();
    if values.is_empty() {
        return Err(XpsFitError::InvalidBackground);
    }
    Ok(values.iter().sum::<f64>() / values.len() as f64)
}

fn linear_background(x: &[f64], low_level: f64, high_level: f64) -> Vec<f64> {
    let low_x = x.iter().copied().fold(f64::INFINITY, f64::min);
    let high_x = x.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    x.iter()
        .map(|value| {
            let t = (*value - low_x) / (high_x - low_x);
            low_level + t * (high_level - low_level)
        })
        .collect()
}

fn shirley_with_levels(
    energy: &[f64],
    intensity: &[f64],
    low_level: f64,
    high_level: f64,
    tolerance: f64,
    max_iterations: usize,
) -> Result<Vec<f64>, XpsFitError> {
    if !tolerance.is_finite() || tolerance <= 0.0 || max_iterations == 0 {
        return Err(XpsFitError::InvalidBackground);
    }
    let flipped = energy[0] < energy[energy.len() - 1];
    let mut x = energy.to_vec();
    let mut y = intensity.to_vec();
    if flipped {
        x.reverse();
        y.reverse();
    }
    let n = x.len();
    let mut background = (0..n)
        .map(|i| high_level + (low_level - high_level) * i as f64 / (n - 1) as f64)
        .collect::<Vec<_>>();
    for _ in 0..max_iterations {
        let old = background.clone();
        let mut cumulative = vec![0.0; n];
        for i in (0..n - 1).rev() {
            let dx = (x[i] - x[i + 1]).abs();
            let a = (y[i] - background[i]).max(0.0);
            let b = (y[i + 1] - background[i + 1]).max(0.0);
            cumulative[i] = cumulative[i + 1] + 0.5 * (a + b) * dx;
        }
        let total = cumulative[0];
        if total <= f64::MIN_POSITIVE {
            break;
        }
        for i in 0..n {
            background[i] = low_level + (high_level - low_level) * cumulative[i] / total;
        }
        let change = background
            .iter()
            .zip(&old)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        if change < tolerance {
            break;
        }
    }
    if flipped {
        background.reverse();
    }
    Ok(background)
}

fn tougaard_u2_background(
    energy: &[f64],
    intensity: &[f64],
    low_level: f64,
    high_level: f64,
    b_ev2: f64,
    c_ev2: f64,
) -> Result<Vec<f64>, XpsFitError> {
    if !b_ev2.is_finite() || !c_ev2.is_finite() || b_ev2 <= 0.0 || c_ev2 <= 0.0 {
        return Err(XpsFitError::InvalidBackground);
    }
    let ascending = energy[0] < energy[energy.len() - 1];
    let mut x = energy.to_vec();
    let mut y = intensity.to_vec();
    if !ascending {
        x.reverse();
        y.reverse();
    }
    let mut background = vec![low_level; x.len()];
    for i in 1..x.len() {
        let mut integral = 0.0;
        for j in 0..i {
            let t0 = x[i] - x[j];
            let t1 = x[i] - x[j + 1];
            let k0 = tougaard_u2_kernel(t0, b_ev2, c_ev2);
            let k1 = tougaard_u2_kernel(t1, b_ev2, c_ev2);
            let s0 = (y[j] - low_level).max(0.0) * k0;
            let s1 = (y[j + 1] - low_level).max(0.0) * k1;
            integral += 0.5 * (s0 + s1) * (x[j + 1] - x[j]).abs();
        }
        background[i] += integral;
    }
    let correction = high_level - background[background.len() - 1];
    let span = x[x.len() - 1] - x[0];
    for (value, &energy) in background.iter_mut().zip(&x) {
        *value += correction * (energy - x[0]) / span;
    }
    if !ascending {
        background.reverse();
    }
    Ok(background)
}

fn tougaard_u2_kernel(loss_ev: f64, b_ev2: f64, c_ev2: f64) -> f64 {
    b_ev2 * loss_ev / (c_ev2 + loss_ev * loss_ev).powi(2)
}

#[cfg(test)]
#[path = "xps/tests.rs"]
mod tests;
