use super::*;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XpsBootstrapOptions {
    pub samples: usize,
    pub seed: u64,
}

impl Default for XpsBootstrapOptions {
    fn default() -> Self {
        Self {
            samples: 500,
            seed: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XpsBootstrapPeak {
    pub id: XpsComponentId,
    pub center_ev: [f64; 3],
    pub fwhm_ev: [f64; 3],
    pub area: [f64; 3],
    pub fraction: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XpsBootstrapResult {
    pub requested: usize,
    pub converged: usize,
    pub seed: u64,
    pub peaks: Vec<XpsBootstrapPeak>,
}

impl XpsBootstrapResult {
    pub fn convergence_fraction(&self) -> f64 {
        self.converged as f64 / self.requested.max(1) as f64
    }
}

pub fn bootstrap_xps_fit(
    base: &XpsFitResult,
    invocation: &XpsFitInvocation,
    options: &XpsBootstrapOptions,
    cancelled: &impl Fn() -> bool,
) -> Result<XpsBootstrapResult, XpsFitError> {
    if !(100..=5_000).contains(&options.samples)
        || base.energy_ev.len() != base.envelope.len()
        || base.energy_ev.len() != base.residual.len()
    {
        return Err(XpsFitError::InvalidInput);
    }
    let mut random = XorShift64::new(options.seed);
    let mut samples = invocation
        .peaks
        .iter()
        .map(|peak| PeakSamples::new(peak.id))
        .collect::<Vec<_>>();
    let mut converged = 0;
    for _ in 0..options.samples {
        if cancelled() {
            return Err(XpsFitError::Cancelled);
        }
        let intensity = base
            .envelope
            .iter()
            .zip(&base.residual)
            .map(|(fit, residual)| {
                let sign = if random.next_u64() & 1 == 0 {
                    -1.0
                } else {
                    1.0
                };
                fit + sign * residual
            })
            .collect::<Vec<_>>();
        match fit_xps_peaks(&base.energy_ev, &intensity, invocation, cancelled) {
            Ok(result) => {
                converged += 1;
                for (sample, peak) in samples.iter_mut().zip(&result.peaks) {
                    sample.center.push(peak.center_ev.value);
                    sample.fwhm.push(peak.fwhm_ev.value);
                    sample.area.push(peak.area.value);
                    sample.fraction.push(peak.fraction.value);
                }
            }
            Err(XpsFitError::Cancelled) => return Err(XpsFitError::Cancelled),
            Err(_) => {}
        }
    }
    if converged == 0 {
        return Err(XpsFitError::DidNotConverge);
    }
    Ok(XpsBootstrapResult {
        requested: options.samples,
        converged,
        seed: options.seed,
        peaks: samples
            .into_iter()
            .map(|sample| XpsBootstrapPeak {
                id: sample.id,
                center_ev: percentiles(sample.center),
                fwhm_ev: percentiles(sample.fwhm),
                area: percentiles(sample.area),
                fraction: percentiles(sample.fraction),
            })
            .collect(),
    })
}

struct PeakSamples {
    id: XpsComponentId,
    center: Vec<f64>,
    fwhm: Vec<f64>,
    area: Vec<f64>,
    fraction: Vec<f64>,
}

impl PeakSamples {
    fn new(id: XpsComponentId) -> Self {
        Self {
            id,
            center: Vec::new(),
            fwhm: Vec::new(),
            area: Vec::new(),
            fraction: Vec::new(),
        }
    }
}

fn percentiles(mut values: Vec<f64>) -> [f64; 3] {
    values.sort_by(f64::total_cmp);
    [
        percentile(&values, 0.025),
        percentile(&values, 0.5),
        percentile(&values, 0.975),
    ]
}

fn percentile(values: &[f64], probability: f64) -> f64 {
    let index = probability * (values.len() - 1) as f64;
    let low = index.floor() as usize;
    let high = index.ceil() as usize;
    let weight = index - low as f64;
    values[low] * (1.0 - weight) + values[high] * weight
}

struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        })
    }

    fn next_u64(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }
}
