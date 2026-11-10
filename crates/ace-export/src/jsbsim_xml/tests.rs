use std::f64::consts::PI;

use ace_model::VehicleModel;
use jsbsim::{Fdm, LoadOptions, MemoryResolver};

use super::propulsion::{ELECTRIC_PROPELLERS, SPEED_OF_SOUND_M_S};
use super::*;

const VEHICLE_JSON: &str = include_str!("../../../../examples/c172/vehicle.json");

fn design() -> FixedWingDesign {
    let model: VehicleModel = serde_json::from_str(VEHICLE_JSON).unwrap();
    FixedWingDesign::from_model(&model).unwrap()
}

/// The same aeroplane with no piston engine in its catalog.
fn electric_design() -> FixedWingDesign {
    FixedWingDesign {
        powerplant: Powerplant::Electric,
        ..design()
    }
}

fn loaded(xml: &str) -> Fdm {
    let mut files = MemoryResolver::new();
    files.insert("aircraft/test/test.xml", xml);
    let mut fdm = Fdm::with_options(LoadOptions::new().with_strict(true)).unwrap();
    fdm.load_model(&files, "aircraft", "test").unwrap();
    fdm
}

#[test]
fn the_document_loads_strictly_with_nothing_for_the_port_to_ignore() {
    let xml = jsbsim_aircraft_xml(&design(), "test").unwrap();
    let mut fdm = loaded(&xml);
    fdm.run_ic().unwrap();
    assert!(fdm.get_property("metrics/Sw-sqft").unwrap() > 170.0);
}

#[test]
fn the_document_is_a_pure_function_of_the_design() {
    let design = design();
    let first = jsbsim_aircraft_xml(&design, "test").unwrap();
    assert_eq!(first, jsbsim_aircraft_xml(&design, "test").unwrap());
    assert!(first.contains("<fdm_config name=\"test\""));
    assert!(first.contains("c172_class_reference"));
}

#[test]
fn the_propeller_curve_reproduces_the_designs_efficiency_at_cruise() {
    for design in [design(), electric_design()] {
        let curve = PropellerCurve::of(&design).unwrap();
        let efficiency = curve.cruise_advance * curve.cruise_ct / curve.cruise_cp;
        assert!((efficiency - design.propeller.cruise_efficiency).abs() < 1e-12);
        let tip_m_s = curve.rated_rev_s * PI * design.propeller.diameter_m;
        let forward = design.cruise_speed_m_s;
        let helical_mach = tip_m_s.hypot(forward) / SPEED_OF_SOUND_M_S;
        assert!(
            helical_mach < design.propeller.tip_mach_limit,
            "{helical_mach}"
        );
    }
}

#[test]
fn a_piston_propeller_absorbs_the_lapsed_power_at_the_rated_speed() {
    let design = design();
    let Powerplant::Piston(piston) = design.powerplant else {
        panic!("{:?}", design.powerplant);
    };
    let curve = PropellerCurve::of(&design).unwrap();
    assert!((curve.rated_rev_s * 60.0 - piston.rated_rpm).abs() < 1e-9);
    let density = design.cruise_density_kg_m3().unwrap();
    let absorbed =
        curve.cruise_cp * density * curve.rated_rev_s.powi(3) * design.propeller.diameter_m.powi(5);
    let expected = design.shaft_power_w * density / 1.225;
    assert!((absorbed - expected).abs() < 1e-6, "{absorbed}");
}

#[test]
fn an_electric_propeller_pair_shares_the_shaft_power() {
    let design = electric_design();
    let curve = PropellerCurve::of(&design).unwrap();
    let density = design.cruise_density_kg_m3().unwrap();
    let absorbed =
        curve.cruise_cp * density * curve.rated_rev_s.powi(3) * design.propeller.diameter_m.powi(5);
    assert!((absorbed * ELECTRIC_PROPELLERS - design.shaft_power_w).abs() < 1e-6);
}

#[test]
fn a_piston_design_is_one_real_engine_and_one_propeller_on_a_fuel_tank() {
    let xml = jsbsim_aircraft_xml(&design(), "test").unwrap();
    assert_eq!(xml.matches("<piston_engine").count(), 1);
    assert_eq!(xml.matches("<propeller").count(), 1);
    assert_eq!(xml.matches("<tank type=\"FUEL\">").count(), 1);
    assert!(!xml.contains("electric_engine"));
    assert!(!xml.contains("second-throttle"));
    let mut fdm = loaded(&xml);
    fdm.run_ic().unwrap();
    assert!(fdm.get_property("propulsion/magneto_cmd").is_ok());
    assert!(fdm.node("propulsion/starter_cmd").unwrap().is_some());
    assert!(fdm.node("fcs/mixture-cmd-norm").unwrap().is_some());
    let contents = fdm.get_property("propulsion/total-fuel-lbs").unwrap();
    assert!(
        (contents / POUNDS_PER_KG - 114.0).abs() < 1e-3,
        "{contents}"
    );
}

#[test]
fn the_tank_holds_the_loaded_fuel_with_room_to_the_declared_capacity() {
    let design = design();
    assert_eq!(design.fuel_capacity_kg, 144.0);
    let xml = jsbsim_aircraft_xml(&design, "test").unwrap();
    let capacity = 144.0 * POUNDS_PER_KG;
    let contents = 114.0 * POUNDS_PER_KG;
    assert!(xml.contains(&format!(
        "<capacity unit=\"LBS\"> {capacity:.4} </capacity>"
    )));
    assert!(xml.contains(&format!(
        "<contents unit=\"LBS\"> {contents:.4} </contents>"
    )));
}

#[test]
fn the_piston_engine_starts_on_its_starter_and_magnetos() {
    let xml = jsbsim_aircraft_xml(&design(), "test").unwrap();
    let mut fdm = loaded(&xml);
    fdm.set_property("ic/h-sl-ft", 4.3).unwrap();
    fdm.run_ic().unwrap();
    fdm.set_property("fcs/mixture-cmd-norm", 1.0).unwrap();
    fdm.set_property("fcs/throttle-cmd-norm", 0.2).unwrap();
    fdm.set_property("fcs/left-brake-cmd-norm", 1.0).unwrap();
    fdm.set_property("fcs/right-brake-cmd-norm", 1.0).unwrap();
    fdm.set_property("propulsion/magneto_cmd", 3.0).unwrap();
    fdm.set_property("propulsion/starter_cmd", 1.0).unwrap();
    for _ in 0..600 {
        fdm.run().unwrap();
    }
    fdm.set_property("propulsion/starter_cmd", 0.0).unwrap();
    for _ in 0..1200 {
        fdm.run().unwrap();
    }
    let running = fdm
        .get_property("propulsion/engine[0]/set-running")
        .unwrap();
    assert_eq!(running, 1.0);
    let rpm = fdm
        .get_property("propulsion/engine[0]/propeller-rpm")
        .unwrap();
    assert!(rpm > 600.0, "{rpm}");
}

#[test]
fn the_electric_pair_runs_from_one_lever_and_its_torques_cancel_from_the_first_step() {
    let xml = jsbsim_aircraft_xml(&electric_design(), "test").unwrap();
    assert_eq!(xml.matches("<electric_engine").count(), 2);
    let mut fdm = loaded(&xml);
    fdm.set_property("ic/vt-fps", 190.0).unwrap();
    fdm.set_property("ic/h-sl-ft", 8_000.0).unwrap();
    fdm.run_ic().unwrap();
    fdm.set_property("fcs/throttle-cmd-norm", 0.6).unwrap();
    for _ in 0..240 {
        fdm.run().unwrap();
        let moment = fdm.get_property("moments/l-prop-lbsft").unwrap();
        assert!(moment.abs() < 1e-6, "{moment}");
    }
    let first = fdm.get_property("propulsion/engine[0]/thrust-lbs").unwrap();
    let second = fdm.get_property("propulsion/engine[1]/thrust-lbs").unwrap();
    assert!(first > 10.0, "{first}");
    assert!((first - second).abs() < 1e-9, "{first} vs {second}");
}

#[test]
fn full_flap_adds_the_landing_polars_lift_and_drag() {
    let design = design();
    assert!((design.flaps.delta_cl - 0.6).abs() < 1e-12);
    assert!((design.flaps.delta_cd - 0.029).abs() < 1e-12);
    let mut fdm = loaded(&jsbsim_aircraft_xml(&design, "test").unwrap());
    fdm.set_property("ic/vt-fps", 190.0).unwrap();
    fdm.set_property("ic/h-sl-ft", 8_000.0).unwrap();
    fdm.run_ic().unwrap();
    fdm.set_property("fcs/flap-cmd-norm", 1.0).unwrap();
    for _ in 0..800 {
        fdm.run().unwrap();
    }
    assert_eq!(fdm.get_property("fcs/flap-pos-norm").unwrap(), 1.0);
    let qbar_s =
        fdm.get_property("aero/qbar-psf").unwrap() * fdm.get_property("metrics/Sw-sqft").unwrap();
    let cl = fdm.get_property("aero/force/lift").unwrap() / qbar_s;
    let cd = fdm.get_property("aero/force/drag").unwrap() / qbar_s;
    let alpha = fdm.get_property("aero/alpha-rad").unwrap();
    let clean_cl = design.lift_slope_per_rad() * (alpha - design.zero_lift_alpha_rad());
    assert!((cl - clean_cl - design.flaps.delta_cl).abs() < 1e-3, "{cl}");
    let clean_cd = design.cd0 + design.induced_drag_factor() * cl * cl;
    assert!((cd - clean_cd - design.flaps.delta_cd).abs() < 1e-3, "{cd}");
}

#[test]
fn the_brakes_reach_the_main_wheels_and_the_nose_has_none() {
    let xml = jsbsim_aircraft_xml(&design(), "test").unwrap();
    assert_eq!(xml.matches("<contact type=\"BOGEY\"").count(), 3);
    assert!(xml.contains("<brake_group> LEFT </brake_group>"));
    assert!(xml.contains("<brake_group> RIGHT </brake_group>"));
    assert!(xml.contains("<brake_group> NONE </brake_group>"));
}

#[test]
fn pitch_trim_moves_the_elevator_without_the_stick() {
    let mut fdm = loaded(&jsbsim_aircraft_xml(&design(), "test").unwrap());
    fdm.set_property("ic/vt-fps", 190.0).unwrap();
    fdm.run_ic().unwrap();
    fdm.set_property("fcs/pitch-trim-cmd-norm", 0.5).unwrap();
    fdm.run().unwrap();
    let elevator = fdm.get_property("fcs/elevator-pos-rad").unwrap();
    assert!((elevator - 0.175).abs() < 1e-9, "{elevator}");
}

#[test]
fn the_lift_table_peaks_at_cl_max_and_breaks_beyond_it() {
    let design = design();
    let xml = jsbsim_aircraft_xml(&design, "test").unwrap();
    let rows: Vec<(f64, f64)> = xml
        .split("<tableData>")
        .find(|section| section.contains("0.00000") && !section.contains("C_THRUST"))
        .map(|_| aero_rows(&xml))
        .unwrap();
    let peak = rows.iter().map(|row| row.1).fold(f64::MIN, f64::max);
    assert!((peak - design.cl_max).abs() < 1e-4);
    let at_peak = rows.iter().position(|row| row.1 == peak).unwrap();
    assert!(rows[at_peak + 1].1 < peak, "the wing stalls");
    assert_eq!(rows[at_peak + 1..].len(), 2);
}

fn aero_rows(xml: &str) -> Vec<(f64, f64)> {
    let lift = xml.split("<axis name=\"LIFT\">").nth(1).unwrap();
    let table = lift.split("<tableData>").nth(1).unwrap();
    let table = table.split("</tableData>").next().unwrap();
    table
        .lines()
        .filter_map(|line| {
            let mut numbers = line.split_whitespace().map(|n| n.parse().unwrap());
            Some((numbers.next()?, numbers.next()?))
        })
        .collect()
}
