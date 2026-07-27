use std::collections::BTreeMap;

use crate::domain::topology::AircraftComponent;

use super::statistical_mass;

fn component(kind: &str, count: u32) -> AircraftComponent {
    AircraftComponent {
        id: kind.to_owned(),
        kind: kind.to_owned(),
        definition: None,
        count,
        roles: Vec::new(),
        parameters: BTreeMap::new(),
    }
}

#[test]
fn statistical_weights_are_category_specific_and_count_aware() {
    let single = component("fuselage", 1);
    let repeated = component("fuselage", 2);
    let light = statistical_mass(&single, "normal_category_light_aircraft");
    let transport = statistical_mass(&single, "transport");
    let doubled = statistical_mass(&repeated, "normal_category_light_aircraft");

    assert_ne!(light.value, transport.value);
    assert_eq!(doubled.value, 2.0 * light.value);
}
