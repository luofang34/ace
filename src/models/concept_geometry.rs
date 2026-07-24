use crate::domain::schema::ResolvedScenario;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ConceptGeometry {
    pub(crate) fuselage_length_m: f64,
    pub(crate) fuselage_width_m: f64,
    pub(crate) fuselage_height_m: f64,
    pub(crate) wing_x_m: f64,
    pub(crate) wing_z_m: f64,
    pub(crate) tail_x_m: f64,
    pub(crate) tail_z_m: f64,
    pub(crate) horizontal_tail_area_m2: f64,
    pub(crate) vertical_tail_area_m2: f64,
    pub(crate) taper_ratio: f64,
}

impl ConceptGeometry {
    pub(crate) fn from_scenario(scenario: &ResolvedScenario) -> Self {
        let wing = &scenario.aircraft.wing;
        let transport = scenario.aircraft.category.contains("transport")
            || scenario
                .aircraft
                .propulsion_architecture
                .contains("turbofan");
        let fuselage_length_m = wing.span_m * if transport { 1.15 } else { 0.75 };
        let fuselage_width_m = wing.area_m2.sqrt() / if transport { 5.0 } else { 3.0 };
        let fuselage_height_m = fuselage_width_m * if transport { 1.05 } else { 1.08 };
        Self {
            fuselage_length_m,
            fuselage_width_m,
            fuselage_height_m,
            wing_x_m: fuselage_length_m * if transport { 0.42 } else { 0.34 },
            wing_z_m: fuselage_height_m * if transport { -0.12 } else { 0.32 },
            tail_x_m: fuselage_length_m * 0.84,
            tail_z_m: fuselage_height_m * 0.15,
            horizontal_tail_area_m2: wing.area_m2 * if transport { 0.24 } else { 0.20 },
            vertical_tail_area_m2: wing.area_m2 * if transport { 0.12 } else { 0.10 },
            taper_ratio: if transport { 0.28 } else { 0.50 },
        }
    }

    pub(crate) fn tail_arm_m(self) -> f64 {
        self.tail_x_m - self.wing_x_m
    }
}
