use crate::domain::result::MassPropertiesState;

use super::MassStateEvidence;

impl From<&MassPropertiesState> for MassStateEvidence {
    fn from(value: &MassPropertiesState) -> Self {
        Self {
            id: value.id.clone(),
            total_mass: value.total_mass.clone(),
            fuel_mass: value.fuel_mass.clone(),
            payload_mass: value.payload_mass.clone(),
            center_of_gravity: value.center_of_gravity.clone(),
            static_margin: value.static_margin,
        }
    }
}
