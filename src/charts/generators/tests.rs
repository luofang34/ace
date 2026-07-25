use crate::domain::aerodynamics::PolarTable;
use crate::test_support::example_scenario;

use super::{drag_polar, performance_curves};

#[test]
fn drag_polar_retains_reference_mach_extrapolation() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let clean = &mut scenario.aircraft.aerodynamics.clean;
    clean.polar_table = Some(PolarTable {
        mach: vec![0.2, 0.8],
        cd0: vec![clean.cd0; 2],
        cl_max: vec![clean.cl_max; 2],
        oswald_efficiency: Some(vec![clean.oswald_efficiency; 2]),
        induced_drag_factor: None,
    });

    let chart = drag_polar(&scenario)?;
    assert!(chart.warnings.iter().any(|warning| {
        warning.code == "MODEL_EXTRAPOLATION"
            && warning.path.as_deref() == Some("aircraft.aerodynamics.clean.polar_table.mach")
    }));
    Ok(())
}

#[test]
fn performance_curves_retain_reference_and_point_extrapolation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let clean = &mut scenario.aircraft.aerodynamics.clean;
    clean.polar_table = Some(PolarTable {
        mach: vec![0.2, 0.8],
        cd0: vec![clean.cd0; 2],
        cl_max: vec![clean.cl_max; 2],
        oswald_efficiency: Some(vec![clean.oswald_efficiency; 2]),
        induced_drag_factor: None,
    });

    let chart = performance_curves(&scenario, 0.0)?;
    assert!(chart.warnings.iter().any(|warning| {
        warning.code == "MODEL_EXTRAPOLATION"
            && warning.path.as_deref() == Some("aircraft.aerodynamics.clean.polar_table.mach")
    }));
    Ok(())
}
