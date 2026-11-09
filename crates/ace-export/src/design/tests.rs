use ace_model::{Inertia, Quantity};

use super::*;

const VEHICLE_JSON: &str = include_str!("../../../../examples/c172/vehicle.json");

fn c172() -> VehicleModel {
    serde_json::from_str(VEHICLE_JSON).unwrap()
}

fn set(model: &mut VehicleModel, name: &str, value: f64) {
    let parameter = model.parameters.get_mut(name).unwrap();
    parameter.value = Some(Quantity {
        value,
        unit: parameter.unit,
    });
    parameter.lower = None;
    parameter.upper = None;
}

fn out_of_range(error: AceExportError) -> &'static str {
    match error {
        AceExportError::OutOfRange { quantity, .. } => quantity,
        other => panic!("{other}"),
    }
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() < tolerance,
        "{actual} vs {expected}"
    );
}

#[test]
fn the_reference_aircraft_reads_as_its_file_states() {
    let design = FixedWingDesign::from_model(&c172()).unwrap();
    assert_eq!(design.id, "c172_class_reference");
    assert_eq!((design.wing_area_m2, design.wing_span_m), (16.17, 11.0));
    assert_eq!(
        (design.cd0, design.oswald, design.cl_max),
        (0.033, 0.8, 1.45)
    );
    assert_eq!(design.empty.mass_kg, 767.0);
    assert_eq!(design.payload.mass_kg, 230.0);
    assert_eq!(design.fuel.mass_kg, 114.0);
    assert_eq!(design.empty.station_m, DVec3::new(3.05, 0.0, 0.0));
    assert_eq!(design.loaded_mass_kg(), 1_111.0);
    assert_eq!(design.propeller.diameter_m, 1.93);
    assert_eq!(design.propeller.blade_count, 2);
    assert_eq!(design.propeller.cruise_efficiency, 0.8);
}

#[test]
fn shaft_power_is_the_models_expression_not_just_the_rating() {
    let mut model = c172();
    assert_eq!(
        FixedWingDesign::from_model(&model).unwrap().shaft_power_w,
        134_000.0
    );
    set(&mut model, "engine.sizing_factor", 1.25);
    close(
        FixedWingDesign::from_model(&model).unwrap().shaft_power_w,
        167_500.0,
        1e-6,
    );
}

#[test]
fn units_are_converted_rather_than_assumed() {
    let mut model = c172();
    let parameter = model.parameters.get_mut("wing.span").unwrap();
    parameter.value = Some(Quantity {
        value: 36.089_238,
        unit: Unit::Foot,
    });
    close(
        FixedWingDesign::from_model(&model).unwrap().wing_span_m,
        11.0,
        1e-4,
    );
}

#[test]
fn the_loaded_centre_of_gravity_is_the_mass_weighted_station() {
    let design = FixedWingDesign::from_model(&c172()).unwrap();
    let expected = (767.0 * 3.05 + 230.0 * 3.25 + 114.0 * 3.15) / 1_111.0;
    close(design.loaded_cg_station_m().x, expected, 1e-12);
}

#[test]
fn the_estimated_inertia_is_of_the_order_of_a_real_172() {
    let design = FixedWingDesign::from_model(&c172()).unwrap();
    let inertia = design.empty_inertia_kg_m2;
    // Published C172 values: Ixx 1285, Iyy 1825, Izz 2667 kg·m².
    close(inertia.x_axis.x, 1_285.0, 300.0);
    close(inertia.y_axis.y, 1_825.0, 300.0);
    close(inertia.z_axis.z, 2_667.0, 400.0);
    assert!(inertia.z_axis.z <= inertia.x_axis.x + inertia.y_axis.y);
}

#[test]
fn a_declared_inertia_replaces_the_estimate_with_its_axes_flipped() {
    let mut model = c172();
    let constant = |value| {
        Binding::Constant(Quantity {
            value,
            unit: Unit::Inertia,
        })
    };
    model.mass_items.get_mut("empty").unwrap().inertia = Some(Inertia {
        xx: constant(1_000.0),
        yy: constant(2_000.0),
        zz: constant(2_500.0),
        xy: constant(10.0),
        xz: constant(20.0),
        yz: constant(30.0),
    });
    let inertia = FixedWingDesign::from_model(&model)
        .unwrap()
        .empty_inertia_kg_m2;
    assert_eq!(inertia.diagonal(), DVec3::new(1_000.0, 2_000.0, 2_500.0));
    assert_eq!(inertia.x_axis.y, -10.0, "x reverses alone");
    assert_eq!(inertia.x_axis.z, 20.0, "x and z reverse together");
    assert_eq!(inertia.y_axis.z, -30.0, "z reverses alone");
}

#[test]
fn aerodynamic_estimates_follow_the_wing() {
    let design = FixedWingDesign::from_model(&c172()).unwrap();
    close(design.aspect_ratio(), 7.483, 1e-3);
    close(design.mean_chord_m(), 1.47, 1e-9);
    close(design.lift_slope_per_rad(), 4.824, 1e-3);
    close(design.induced_drag_factor(), 1.0 / (PI * 0.8 * 7.483), 1e-3);
}

#[test]
fn the_cruise_angle_carries_the_weight_at_cruise_density() {
    let design = FixedWingDesign::from_model(&c172()).unwrap();
    let density = design.cruise_density_kg_m3().unwrap();
    close(density, 0.962, 0.005);
    let alpha = design.cruise_alpha_rad().unwrap();
    let lift_coefficient = (alpha - design.zero_lift_alpha_rad()) * design.lift_slope_per_rad();
    let speed = design.cruise_speed_m_s;
    let lift = lift_coefficient * 0.5 * density * speed * speed * design.wing_area_m2;
    close(lift, 1_111.0 * STANDARD_GRAVITY_M_S2, 1e-6);
    assert!(lift_coefficient < design.cl_max);
}

#[test]
fn another_domain_has_no_flight_model() {
    let mut model = c172();
    model.domain = "rotorcraft".to_owned();
    assert!(matches!(
        FixedWingDesign::from_model(&model),
        Err(AceExportError::UnsupportedDomain { domain }) if domain == "rotorcraft"
    ));
}

#[test]
fn a_model_that_fails_its_own_validation_is_refused() {
    let mut model = c172();
    model.schema_version = 2;
    assert!(matches!(
        FixedWingDesign::from_model(&model),
        Err(AceExportError::InvalidModel(_))
    ));
}

#[test]
fn nonsensical_parameters_are_refused_by_name() {
    for (name, value, quantity) in [
        ("wing.area", 0.0, "wing.area"),
        ("wing.span", -1.0, "wing.span"),
        ("aero.oswald", 1.2, "aero.oswald"),
        ("aero.cd0", 0.0, "aero.cd0"),
        ("engine.count", 2.0, "engine.count"),
        ("propeller.efficiency", 1.0, "propeller.efficiency"),
        ("fuel.loaded", 500.0, "loaded mass"),
    ] {
        let mut model = c172();
        set(&mut model, name, value);
        let error = FixedWingDesign::from_model(&model).unwrap_err();
        assert_eq!(out_of_range(error), quantity, "{name}");
    }
}

#[test]
fn a_mass_statement_without_fuel_is_incomplete() {
    let mut model = c172();
    model.mass_items.remove("fuel");
    for analysis in ["mission", "loading"] {
        model.analyses.remove(analysis);
    }
    let error = FixedWingDesign::from_model(&model).unwrap_err();
    assert!(
        matches!(
            error,
            AceExportError::MissingMassItem { kind: "fuel" } | AceExportError::InvalidModel(_)
        ),
        "{error}"
    );
}

#[test]
fn a_mass_bound_to_an_analysis_output_has_no_value_to_compile() {
    let mut model = c172();
    model.mass_items.get_mut("payload").unwrap().mass =
        Binding::Parameter("mission.block_fuel".to_owned());
    assert!(matches!(
        FixedWingDesign::from_model(&model),
        Err(AceExportError::UnresolvedParameter { name }) if name == "mission.block_fuel"
    ));
}

#[test]
fn propeller_data_must_be_there_and_in_metres() {
    let mut model = c172();
    let entry = model.catalog.get_mut("propeller").unwrap();
    entry.document["parameters"]["diameter"] = "76 in".into();
    assert!(matches!(
        FixedWingDesign::from_model(&model),
        Err(AceExportError::Catalog {
            field: "diameter",
            ..
        })
    ));
    let mut model = c172();
    model.catalog.get_mut("propeller").unwrap().kind = "rotor".to_owned();
    let error = FixedWingDesign::from_model(&model).unwrap_err();
    assert!(
        matches!(error, AceExportError::Catalog { field: "kind", .. }),
        "{error}"
    );
}

#[test]
fn the_catalog_says_what_turns_the_propeller() {
    let design = FixedWingDesign::from_model(&c172()).unwrap();
    assert_eq!(
        design.powerplant,
        Powerplant::Piston(PistonDesign {
            rated_rpm: 2_700.0,
            power_lapse_exponent: 1.0,
        })
    );
    let mut model = c172();
    model.catalog.get_mut("engine").unwrap().kind = "electric_motor".to_owned();
    let electric = FixedWingDesign::from_model(&model).unwrap();
    assert_eq!(electric.powerplant, Powerplant::Electric);
}

#[test]
fn a_design_with_no_polars_gets_a_plain_flap() {
    let mut model = c172();
    model.catalog.get_mut("polars").unwrap().kind = "unread_polars".to_owned();
    let design = FixedWingDesign::from_model(&model).unwrap();
    assert!(design.flaps.delta_cl > 0.0 && design.flaps.delta_cd > 0.0);
}

#[test]
fn the_standard_atmosphere_matches_its_published_table() {
    for (height_m, density) in [
        (0.0, 1.2250),
        (1_000.0, 1.1117),
        (5_000.0, 0.7364),
        (11_000.0, 0.3648),
        (15_000.0, 0.1948),
    ] {
        let read = atmosphere::isa_density_kg_m3(height_m).unwrap();
        assert!((read - density).abs() < 1e-4, "{height_m}: {read}");
    }
    assert_eq!(atmosphere::isa_density_kg_m3(30_000.0), None);
}
