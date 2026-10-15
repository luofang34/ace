use super::*;

const SIGMA_5500: f64 = 0.5793;

fn turbo(floor: Option<f64>) -> PowerLapse {
    PowerLapse::CriticalAltitude {
        critical_altitude_m: 5500.0,
        critical_density_ratio: SIGMA_5500,
        exponent_below: 0.0,
        exponent_above: 1.0,
        minimum_power_fraction: floor,
    }
}

#[test]
fn critical_altitude_holds_rated_power_then_lapses_continuously() {
    let lapse = turbo(None);
    assert_eq!(lapse.fraction(1.0).fraction, 1.0);
    assert_eq!(lapse.fraction(SIGMA_5500 + 1e-9).fraction, 1.0);
    assert!((lapse.fraction(SIGMA_5500 - 1e-9).fraction - 1.0).abs() < 1e-8);
    // 11 km is ~36% sea-level density: power follows density above 5.5 km.
    let high = lapse.fraction(0.3639);
    assert!((high.fraction - 0.3639 / SIGMA_5500).abs() < 1e-12);
    assert!(!high.floored);
}

#[test]
fn floors_report_when_they_supply_the_power() {
    let floored = turbo(Some(0.7)).fraction(0.3639);
    assert_eq!(floored.fraction, 0.7);
    assert!(floored.floored);
    let aspirated = PowerLapse::DensityPower {
        exponent: 1.0,
        minimum_power_fraction: None,
    };
    let point = aspirated.fraction(0.3639);
    assert_eq!(point.fraction, 0.3639);
    assert!(!point.floored);
}

#[test]
fn bsfc_altitude_slope_is_declared_and_linear() {
    let mut bsfc = ShaftBsfc {
        takeoff_kg_kwh: 0.29,
        cruise_kg_kwh: 0.25,
        economy_kg_kwh: 0.23,
        altitude_slope_per_km: None,
    };
    assert_eq!(bsfc.at(PropulsionMode::Cruise, 6000.0), 0.25);
    assert_eq!(bsfc.at(PropulsionMode::Climb, 0.0), 0.29);
    bsfc.altitude_slope_per_km = Some(0.02);
    assert!((bsfc.at(PropulsionMode::Economy, 5000.0) - 0.23 * 1.1).abs() < 1e-12);
}
