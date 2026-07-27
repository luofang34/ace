use crate::domain::schema::ResolvedScenario;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ConceptGeometry {
    pub(crate) fuselage_length_m: f64,
    pub(crate) fuselage_width_m: f64,
    pub(crate) fuselage_height_m: f64,
    pub(crate) wing_x_m: f64,
    pub(crate) wing_z_m: f64,
    pub(crate) horizontal_tail_x_m: f64,
    pub(crate) vertical_tail_x_m: f64,
    pub(crate) tail_z_m: f64,
    pub(crate) horizontal_tail_area_m2: f64,
    pub(crate) vertical_tail_area_m2: f64,
    pub(crate) taper_ratio: f64,
}

impl ConceptGeometry {
    pub(crate) fn from_scenario(scenario: &ResolvedScenario) -> Self {
        let transport = scenario.aircraft.category.contains("transport")
            || scenario
                .aircraft
                .propulsion_architecture
                .contains("turbofan");
        let geometry = &scenario.aircraft.geometry;
        let fuselage_length_m = geometry
            .fuselage
            .as_ref()
            .map_or(0.0, |fuselage| fuselage.length.value);
        let fuselage_width_m = geometry
            .fuselage
            .as_ref()
            .map_or(0.0, |fuselage| fuselage.diameter.value);
        let fuselage_height_m = fuselage_width_m * if transport { 1.05 } else { 1.08 };
        let wing_x_m = fuselage_length_m * if transport { 0.42 } else { 0.34 };
        Self {
            fuselage_length_m,
            fuselage_width_m,
            fuselage_height_m,
            wing_x_m,
            wing_z_m: fuselage_height_m * if transport { -0.12 } else { 0.32 },
            horizontal_tail_x_m: geometry
                .horizontal_tail
                .as_ref()
                .map_or(wing_x_m, |tail| wing_x_m + tail.arm.value),
            vertical_tail_x_m: geometry
                .vertical_tail
                .as_ref()
                .map_or(wing_x_m, |tail| wing_x_m + tail.arm.value),
            tail_z_m: fuselage_height_m * 0.15,
            horizontal_tail_area_m2: geometry
                .horizontal_tail
                .as_ref()
                .map_or(0.0, |tail| tail.area.value),
            vertical_tail_area_m2: geometry
                .vertical_tail
                .as_ref()
                .map_or(0.0, |tail| tail.area.value),
            taper_ratio: if transport { 0.28 } else { 0.50 },
        }
    }
}
