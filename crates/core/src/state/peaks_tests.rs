use super::*;

fn trace(ys: Vec<f64>) -> Trace1d {
    Trace1d {
        xs: (0..ys.len()).map(|i| i as f64).collect(),
        ys,
        x_reversed: false,
    }
}

/// A strong apex at x = 5 and a weak one at x = 10.
fn two_peaks() -> Trace1d {
    let mut ys = vec![0.0; 16];
    ys[4] = 40.0;
    ys[5] = 100.0;
    ys[6] = 40.0;
    ys[9] = 2.0;
    ys[10] = 5.0;
    ys[11] = 2.0;
    trace(ys)
}

#[test]
fn a_narrow_window_picks_the_weak_apex_beside_a_strong_one() {
    let trace = two_peaks();
    // Clicking at the weak line while zoomed in: the pixel-derived window no
    // longer reaches the strong apex.
    assert_eq!(trace.snap_within(10.4, 2.0), (10.0, 5.0));
}

#[test]
fn a_wide_window_still_lands_on_the_tallest_apex() {
    let trace = two_peaks();
    // Zoomed out, the same click may sit several samples off; the tallest
    // apex in reach is the intended target, not the nearest noise wiggle.
    assert_eq!(trace.snap_within(7.0, 6.0), (5.0, 100.0));
}

#[test]
fn free_placement_takes_the_nearest_sample_without_an_apex_search() {
    let trace = two_peaks();
    assert_eq!(trace.pick(9.4, ManualPeakSnap::NearestSample), (9.0, 2.0));
}

#[test]
fn an_empty_window_falls_back_to_the_nearest_sample() {
    let trace = two_peaks();
    // No local maximum within reach of x = 14.
    assert_eq!(trace.snap_within(14.2, 1.0), (14.0, 0.0));
}

#[test]
fn apex_snap_routes_through_pick() {
    let trace = two_peaks();
    assert_eq!(
        trace.pick(10.4, ManualPeakSnap::Apex { half_width: 2.0 }),
        (10.0, 5.0)
    );
}
