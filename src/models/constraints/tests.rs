use crate::domain::aerodynamics::PolarTable;
use crate::test_support::example_scenario;

use super::ConstraintAnalyzer;

#[test]
fn constraint_analysis_retains_each_configuration_extrapolation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    for configuration in [
        &mut scenario.aircraft.aerodynamics.clean,
        &mut scenario.aircraft.aerodynamics.takeoff,
        &mut scenario.aircraft.aerodynamics.landing,
    ] {
        configuration.polar_table = Some(PolarTable {
            mach: vec![0.5, 0.8],
            cd0: vec![configuration.cd0; 2],
            cl_max: vec![configuration.cl_max; 2],
            oswald_efficiency: Some(vec![configuration.oswald_efficiency; 2]),
            induced_drag_factor: None,
        });
    }
    let result = ConstraintAnalyzer::new(scenario).analyze(1_000.0, 2_000.0, 5)?;
    let paths = result
        .warnings
        .iter()
        .filter(|warning| warning.code == "MODEL_EXTRAPOLATION")
        .filter_map(|warning| warning.path.as_deref())
        .collect::<Vec<_>>();

    assert_eq!(
        paths,
        [
            "aircraft.aerodynamics.clean.polar_table.mach",
            "aircraft.aerodynamics.takeoff.polar_table.mach",
            "aircraft.aerodynamics.landing.polar_table.mach",
        ]
    );
    Ok(())
}
