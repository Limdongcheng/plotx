use super::*;

fn free(initial: f64, bounds: [f64; 2]) -> PeakParameterConstraint {
    PeakParameterConstraint::Free { initial, bounds }
}

fn fixed(value: f64) -> PeakParameterConstraint {
    PeakParameterConstraint::Fixed { value }
}

fn options() -> ConstrainedLineShapeOptions {
    ConstrainedLineShapeOptions {
        shape: LineShape::PseudoVoigt,
        pseudo_voigt_fraction: 0.3,
        max_iterations: 500,
    }
}

fn axis() -> Vec<f64> {
    (0..241).map(|index| 280.0 + index as f64 * 0.05).collect()
}

#[test]
fn constrained_fit_preserves_keys_and_links_after_reordering() {
    let x = axis();
    let first = PeakConstraintKey(11);
    let second = PeakConstraintKey(22);
    let y = x
        .iter()
        .map(|&x| {
            area_normalized_peak(LineShape::PseudoVoigt, 0.3, x, 285.0, 1.2, 100.0)
                + area_normalized_peak(LineShape::PseudoVoigt, 0.3, x, 288.0, 1.2, 50.0)
        })
        .collect::<Vec<_>>();
    let mut specs = vec![
        ConstrainedPeakSpec {
            key: first,
            position: free(285.1, [284.5, 285.5]),
            fwhm: free(1.1, [0.8, 2.0]),
            area: free(90.0, [0.0, 200.0]),
        },
        ConstrainedPeakSpec {
            key: second,
            position: PeakParameterConstraint::Linked {
                reference: first,
                scale: 1.0,
                offset: 3.0,
            },
            fwhm: PeakParameterConstraint::Linked {
                reference: first,
                scale: 1.0,
                offset: 0.0,
            },
            area: PeakParameterConstraint::Linked {
                reference: first,
                scale: 0.5,
                offset: 0.0,
            },
        },
    ];
    specs.reverse();
    let result = fit_constrained_lineshapes(&x, &y, &specs, options(), &|| false).unwrap();
    let main = result.peaks.iter().find(|peak| peak.key == first).unwrap();
    let linked = result.peaks.iter().find(|peak| peak.key == second).unwrap();
    assert!((linked.position - main.position - 3.0).abs() < 1e-10);
    assert!((linked.fwhm - main.fwhm).abs() < 1e-10);
    assert!((linked.area / main.area - 0.5).abs() < 1e-10);
    assert!(result.physical_covariance.is_some());
}

#[test]
fn fixed_fit_and_cancellation_use_the_common_path() {
    let x = axis();
    let y = x
        .iter()
        .map(|&x| area_normalized_peak(LineShape::PseudoVoigt, 0.3, x, 285.0, 1.2, 100.0))
        .collect::<Vec<_>>();
    let specs = vec![ConstrainedPeakSpec {
        key: PeakConstraintKey(1),
        position: fixed(285.0),
        fwhm: fixed(1.2),
        area: fixed(100.0),
    }];
    let result = fit_constrained_lineshapes(&x, &y, &specs, options(), &|| false).unwrap();
    assert!(result.physical_covariance.is_none());
    assert!(result.residual.iter().all(|value| value.abs() < 1e-12));
    assert_eq!(
        fit_constrained_lineshapes(&x, &y, &specs, options(), &|| true),
        Err(ConstrainedLineFitError::Cancelled)
    );
}

#[test]
fn invalid_graph_and_linked_physical_values_are_rejected() {
    let key = PeakConstraintKey(1);
    let missing = ConstrainedPeakSpec {
        key,
        position: PeakParameterConstraint::Linked {
            reference: PeakConstraintKey(2),
            scale: 1.0,
            offset: 0.0,
        },
        fwhm: fixed(1.0),
        area: fixed(1.0),
    };
    assert!(matches!(
        validate_constrained_peaks(&[missing]),
        Err(ConstrainedLineFitError::InvalidConstraints(_))
    ));

    let specs = vec![
        ConstrainedPeakSpec {
            key,
            position: fixed(1.0),
            fwhm: fixed(1.0),
            area: fixed(1.0),
        },
        ConstrainedPeakSpec {
            key: PeakConstraintKey(2),
            position: fixed(2.0),
            fwhm: PeakParameterConstraint::Linked {
                reference: key,
                scale: -1.0,
                offset: 0.0,
            },
            area: fixed(1.0),
        },
    ];
    let x = (0..8).map(|value| value as f64).collect::<Vec<_>>();
    assert!(matches!(
        fit_constrained_lineshapes(&x, &[0.0; 8], &specs, options(), &|| false),
        Err(ConstrainedLineFitError::InvalidConstraints(_))
    ));
}
