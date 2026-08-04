use super::*;

fn axis() -> Vec<f64> {
    (0..241).map(|index| 292.0 - index as f64 * 0.05).collect()
}

fn invocation(x: &[f64]) -> XpsFitInvocation {
    let first = XpsComponentId::new(1);
    XpsFitInvocation {
        background: XpsBackgroundSpec::suggested(x).unwrap(),
        peaks: vec![
            XpsPeakSpec {
                id: first,
                label: "main".into(),
                center: XpsCenterConstraint::Free {
                    initial_ev: 285.1,
                    bounds_ev: [284.5, 285.5],
                },
                fwhm: XpsFwhmConstraint::Free {
                    initial_ev: 1.1,
                    bounds_ev: [0.8, 2.0],
                },
                area: XpsAreaConstraint::Free {
                    initial: 100.0,
                    bounds: [0.0, 1_000.0],
                },
            },
            XpsPeakSpec {
                id: XpsComponentId::new(2),
                label: "linked".into(),
                center: XpsCenterConstraint::Offset {
                    reference: first,
                    delta_ev: 3.0,
                },
                fwhm: XpsFwhmConstraint::Shared { reference: first },
                area: XpsAreaConstraint::Ratio {
                    reference: first,
                    ratio: 0.5,
                },
            },
        ],
        options: XpsFitOptions::default(),
    }
}

#[test]
fn backgrounds_are_order_independent() {
    let x = axis();
    let y = x
        .iter()
        .map(|value| 5.0 + gl_peak(*value, 285.0, 1.2, 100.0, 0.3))
        .collect::<Vec<_>>();
    for model in [
        XpsBackgroundModel::Linear,
        XpsBackgroundModel::default(),
        XpsBackgroundModel::TougaardU2 {
            b_ev2: 3_000.0,
            c_ev2: 1_643.0,
        },
    ] {
        let mut spec = XpsBackgroundSpec::suggested(&x).unwrap();
        spec.model = model;
        let forward = compute_xps_background(&x, &y, &spec).unwrap();
        let mut xr = x.clone();
        let mut yr = y.clone();
        xr.reverse();
        yr.reverse();
        let mut reverse = compute_xps_background(&xr, &yr, &spec).unwrap().background;
        reverse.reverse();
        assert!(
            forward
                .background
                .iter()
                .zip(reverse)
                .all(|(a, b)| (a - b).abs() < 1e-8)
        );
    }
}

#[test]
fn tougaard_u2_anchored_trapezoid_regression() {
    let x = (0..8).map(|value| value as f64).collect::<Vec<_>>();
    let y = vec![2.0, 4.0, 8.0, 5.0, 3.0, 2.0, 2.0, 2.0];
    let spec = XpsBackgroundSpec {
        model: XpsBackgroundModel::TougaardU2 {
            b_ev2: 2.0,
            c_ev2: 3.0,
        },
        window_ev: [0.0, 7.0],
        low_anchor_ev: [0.0, 0.0],
        high_anchor_ev: [7.0, 7.0],
    };
    let expected = [
        2.0,
        1.971_363_090_560_919,
        2.192_726_181_121_837,
        2.827_354_577_805_206,
        2.833_581_613_944_356,
        2.521_034_741_628_157,
        2.193_285_389_428_038,
        2.0,
    ];
    let result = compute_xps_background(&x, &y, &spec).unwrap();
    assert!(
        result
            .background
            .iter()
            .zip(expected)
            .all(|(actual, expected)| (actual - expected).abs() < 1e-12)
    );
}

#[test]
fn tougaard_u2_kernel_matches_quases_closed_form_integral() {
    // QUASES-Tougaard 5.1 User's Guide, eq. (1.7):
    // K(T) = B*T/(C+T^2)^2. Its integral from 0 to L is the expression below.
    let b = 3_000.0;
    let c = 1_643.0;
    let limit: f64 = 100.0;
    let points = 100_000;
    let step = limit / points as f64;
    let numeric = (0..points)
        .map(|index| {
            let low = index as f64 * step;
            let high = low + step;
            0.5 * (tougaard_u2_kernel(low, b, c) + tougaard_u2_kernel(high, b, c)) * step
        })
        .sum::<f64>();
    let reference = 0.5 * b * (1.0 / c - 1.0 / (c + limit.powi(2)));
    assert!((numeric - reference).abs() < 1e-10);
}

#[test]
fn gl_peak_integrates_to_requested_area() {
    let x = (0..20001)
        .map(|i| 190.0 + i as f64 * 0.01)
        .collect::<Vec<_>>();
    let y = x
        .iter()
        .map(|value| gl_peak(*value, 290.0, 1.2, 42.0, 0.3))
        .collect::<Vec<_>>();
    let area = x
        .windows(2)
        .zip(y.windows(2))
        .map(|(a, b)| (a[1] - a[0]) * 0.5 * (b[0] + b[1]))
        .sum::<f64>();
    assert!((area - 42.0).abs() < 0.1);
}

#[test]
fn linked_constraints_and_covariance_are_reported() {
    let x = axis();
    let y = x
        .iter()
        .map(|value| {
            5.0 + gl_peak(*value, 285.0, 1.3, 120.0, 0.3) + gl_peak(*value, 288.0, 1.3, 60.0, 0.3)
        })
        .collect::<Vec<_>>();
    let result = fit_xps_peaks(&x, &y, &invocation(&x), &|| false).unwrap();
    assert!(
        (result.peaks[1].center_ev.value - result.peaks[0].center_ev.value - 3.0).abs() < 1e-10
    );
    assert!((result.peaks[1].fwhm_ev.value - result.peaks[0].fwhm_ev.value).abs() < 1e-10);
    assert!((result.peaks[1].area.value / result.peaks[0].area.value - 0.5).abs() < 1e-10);
    assert!(result.peaks[0].center_ev.standard_error.is_some());
    assert!(result.parameter_correlation.is_some());

    let mut reordered = invocation(&x);
    reordered.peaks.reverse();
    let reordered = fit_xps_peaks(&x, &y, &reordered, &|| false).unwrap();
    let linked = reordered
        .peaks
        .iter()
        .find(|peak| peak.id == XpsComponentId::new(2))
        .unwrap();
    let main = reordered
        .peaks
        .iter()
        .find(|peak| peak.id == XpsComponentId::new(1))
        .unwrap();
    assert!((linked.center_ev.value - main.center_ev.value - 3.0).abs() < 1e-10);
}

#[test]
fn cyclic_constraints_are_rejected() {
    let x = axis();
    let y = vec![1.0; x.len()];
    let mut invocation = invocation(&x);
    invocation.peaks[0].fwhm = XpsFwhmConstraint::Shared {
        reference: invocation.peaks[1].id,
    };
    assert!(matches!(
        fit_xps_peaks(&x, &y, &invocation, &|| false),
        Err(XpsFitError::InvalidConstraints(_))
    ));
}

#[test]
fn missing_self_and_incompatible_bounds_are_rejected() {
    let x = axis();
    let mut spec = invocation(&x);
    spec.peaks[1].center = XpsCenterConstraint::Offset {
        reference: XpsComponentId::new(99),
        delta_ev: 1.0,
    };
    assert!(matches!(
        validate_xps_constraints(&spec),
        Err(XpsFitError::InvalidConstraints(_))
    ));
    spec = invocation(&x);
    spec.peaks[0].fwhm = XpsFwhmConstraint::Shared {
        reference: spec.peaks[0].id,
    };
    assert!(matches!(
        validate_xps_constraints(&spec),
        Err(XpsFitError::InvalidConstraints(_))
    ));
    spec = invocation(&x);
    spec.peaks[0].center = XpsCenterConstraint::Free {
        initial_ev: 285.0,
        bounds_ev: [286.0, 284.0],
    };
    assert!(matches!(
        validate_xps_constraints(&spec),
        Err(XpsFitError::InvalidConstraints(_))
    ));
}

#[test]
fn fixed_only_fit_degrades_without_covariance() {
    let x = axis();
    let y = x
        .iter()
        .map(|value| 5.0 + gl_peak(*value, 285.0, 1.2, 100.0, 0.3))
        .collect::<Vec<_>>();
    let mut spec = invocation(&x);
    spec.peaks.truncate(1);
    spec.peaks[0].center = XpsCenterConstraint::Fixed { value_ev: 285.0 };
    spec.peaks[0].fwhm = XpsFwhmConstraint::Fixed { value_ev: 1.2 };
    spec.peaks[0].area = XpsAreaConstraint::Fixed { value: 100.0 };
    let result = fit_xps_peaks(&x, &y, &spec, &|| false).unwrap();
    assert!(result.parameter_correlation.is_none());
    assert!(result.peaks[0].center_ev.standard_error.is_none());
}

#[test]
fn bootstrap_is_deterministic_and_cancellable() {
    let x = axis();
    let invocation = invocation(&x);
    let y = x
        .iter()
        .map(|value| {
            5.0 + gl_peak(*value, 285.0, 1.3, 120.0, 0.3) + gl_peak(*value, 288.0, 1.3, 60.0, 0.3)
        })
        .collect::<Vec<_>>();
    let fit = fit_xps_peaks(&x, &y, &invocation, &|| false).unwrap();
    let options = XpsBootstrapOptions {
        samples: 100,
        seed: 42,
    };
    let first = bootstrap_xps_fit(&fit, &invocation, &options, &|| false).unwrap();
    let second = bootstrap_xps_fit(&fit, &invocation, &options, &|| false).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        bootstrap_xps_fit(&fit, &invocation, &options, &|| true),
        Err(XpsFitError::Cancelled)
    );
}
