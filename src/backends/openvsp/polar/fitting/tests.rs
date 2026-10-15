use super::*;

fn reference() -> Vec<Point> {
    // Independent elliptic lifting-line reference: CDi = CL^2/(pi*AR), AR=10.
    MACH.into_iter()
        .flat_map(|mach| {
            [0., 2., 4., 6., 8., 10.].into_iter().map(move |alpha| {
                let cl = alpha * 0.1;
                Point {
                    mach,
                    alpha,
                    cl,
                    cdi: cl * cl / (std::f64::consts::PI * 10.0),
                }
            })
        })
        .collect()
}

#[test]
fn quadratic_reduction_recovers_lifting_line_reference() {
    for fit in fit(&reference()).unwrap() {
        assert!((fit.k - 1.0 / (std::f64::consts::PI * 10.0)).abs() < 1e-14);
        assert_eq!(fit.cl_limit, 1.0);
        assert!(fit.maximum_absolute_error < 1e-14);
    }
}

#[test]
fn missing_duplicate_nonlinear_or_nonfinite_samples_cannot_publish() {
    let mut points = reference();
    points.pop();
    assert!(fit(&points).is_err());
    let mut points = reference();
    points[3] = points[2].clone();
    assert!(fit(&points).is_err());
    let mut points = reference();
    points[4].cdi *= 1.3;
    assert!(fit(&points).is_err());
    assert!(parse("ACE_POLAR=0.2,0,NaN,0").is_err());
    assert!(parse("ACE_POLAR=0.2,0,0").is_err());
}

#[test]
fn induced_drag_ports_name_each_sampled_mach_in_order() {
    let names = MACH
        .iter()
        .map(|mach| format!("k_{:03.0}", mach * 100.0))
        .collect::<Vec<_>>();
    assert_eq!(names, super::super::PORTS[..MACH.len()]);
    assert_eq!(super::super::PORTS.last(), Some(&"cl_limit"));
    assert_eq!(MACH.first(), Some(&0.0));
}
