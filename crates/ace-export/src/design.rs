use std::collections::BTreeMap;
use std::f64::consts::PI;

use ace_model::{Binding, MassItem, MassKind, Quantity, Unit, VehicleModel};
use glam::{DMat3, DVec3};

use crate::AceExportError;

mod atmosphere;
mod catalog;

pub use catalog::{FlapDesign, PistonDesign, Powerplant, PropellerDesign};

#[cfg(test)]
mod tests;

/// Domain of the only models with a flight model here.
const FIXED_WING: &str = "fixed_wing";
/// Fuselage length over span, typical of a light aeroplane: the model states
/// no length, and only the inertia estimate needs one.
const LENGTH_OVER_SPAN: f64 = 0.75;
/// Non-dimensional radii of gyration of a single-engine propeller aeroplane
/// (Roskam, Airplane Design Part V), used when the model gives no inertia:
/// `Ixx = m (b Rx / 2)²`, `Iyy = m (L Ry / 2)²`,
/// `Izz = m ((b + L) Rz / 4)²`.
const RADIUS_OF_GYRATION: DVec3 = DVec3::new(0.25, 0.38, 0.39);
/// Lift-curve slope of a thin aerofoil section, per radian.
const SECTION_LIFT_SLOPE: f64 = 2.0 * PI;
/// Zero-lift angle of a cambered light-aeroplane wing, radians.
const ZERO_LIFT_ALPHA_RAD: f64 = -0.044;
/// Standard gravity, m/s².
const STANDARD_GRAVITY_M_S2: f64 = 9.806_65;

/// A mass and where it sits, in the model's axes: `x` aft from the datum, `y`
/// right, `z` up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassStation {
    /// Mass, kg.
    pub mass_kg: f64,
    /// Station, metres.
    pub station_m: DVec3,
}

/// What a fixed-wing [`VehicleModel`] says about how the aircraft flies.
///
/// Everything here is read from the model except the inertia, when the empty
/// mass item declares none, the lift curve's slope and zero-lift angle, which
/// come from aspect ratio and a typical camber, and the flaps, when the catalog
/// has no polars. The model states no stability derivatives or control powers
/// at all, so the JSBSim aircraft uses representative light-aeroplane values
/// for those, and it states no landing gear, so that is representative too.
#[derive(Debug, Clone, PartialEq)]
pub struct FixedWingDesign {
    /// The model's id.
    pub id: String,
    /// The model's display name.
    pub name: String,
    /// `wing.area`, m².
    pub wing_area_m2: f64,
    /// `wing.span`, metres.
    pub wing_span_m: f64,
    /// `aero.cd0`.
    pub cd0: f64,
    /// `aero.oswald`.
    pub oswald: f64,
    /// `aero.cl_max`.
    pub cl_max: f64,
    /// `engine.shaft_power`, W.
    pub shaft_power_w: f64,
    /// What turns the propeller.
    pub powerplant: Powerplant,
    /// The installed propeller.
    pub propeller: PropellerDesign,
    /// What full flap adds to the clean wing.
    pub flaps: FlapDesign,
    /// `phase.cruise.speed`, m/s.
    pub cruise_speed_m_s: f64,
    /// `phase.cruise.altitude`, metres.
    pub cruise_altitude_m: f64,
    /// The operating empty aircraft.
    pub empty: MassStation,
    /// The payload as loaded.
    pub payload: MassStation,
    /// The fuel as loaded.
    pub fuel: MassStation,
    /// What the tank holds when full, kg; the fuel item's capacity, or the
    /// loaded fuel when it declares none.
    pub fuel_capacity_kg: f64,
    /// Inertia of `empty` about its own centre of gravity in forward-right-down
    /// axes, kg·m².
    pub empty_inertia_kg_m2: DMat3,
}

/// Literal parameter values, the scope expressions evaluate against.
pub(crate) struct Values<'a> {
    model: &'a VehicleModel,
    literals: BTreeMap<String, Quantity>,
}

impl<'a> Values<'a> {
    fn of(model: &'a VehicleModel) -> Self {
        let literals = model
            .parameters
            .iter()
            .filter_map(|(name, parameter)| Some((name.clone(), parameter.value?)))
            .collect();
        Self { model, literals }
    }

    pub(crate) fn parameter(&self, name: &str, unit: Unit) -> Result<f64, AceExportError> {
        let error = |source| AceExportError::Parameter {
            name: name.to_owned(),
            source,
        };
        let parameter = self.model.parameter(name).map_err(error)?;
        let quantity = match (parameter.value, &parameter.expression) {
            (Some(value), _) => value.to(unit),
            (None, Some(expression)) => expression.evaluate(&self.literals, unit),
            (None, None) => {
                return Err(AceExportError::UnresolvedParameter {
                    name: name.to_owned(),
                });
            }
        };
        Ok(quantity.map_err(error)?.value)
    }

    fn binding(&self, binding: &Binding, unit: Unit) -> Result<f64, AceExportError> {
        match binding {
            Binding::Parameter(name) => self.parameter(name, unit),
            Binding::Constant(value) => {
                value
                    .to(unit)
                    .map(|q| q.value)
                    .map_err(|source| AceExportError::Parameter {
                        name: "constant binding".to_owned(),
                        source,
                    })
            }
        }
    }

    fn mass_station(&self, item: &MassItem) -> Result<MassStation, AceExportError> {
        let [x, y, z] = &item.cg;
        Ok(MassStation {
            mass_kg: self.binding(&item.mass, Unit::Kilogram)?,
            station_m: DVec3::new(
                self.binding(x, Unit::Meter)?,
                self.binding(y, Unit::Meter)?,
                self.binding(z, Unit::Meter)?,
            ),
        })
    }

    /// The one item of `kind`. Several would each need a station of their
    /// own, which the synthesised tree does not have.
    fn single_item(
        &self,
        kind: MassKind,
        label: &'static str,
    ) -> Result<&'a MassItem, AceExportError> {
        let mut items = self.model.mass_items.values().filter(|i| i.kind == kind);
        match (items.next(), items.next()) {
            (Some(item), None) => Ok(item),
            (None, _) => Err(AceExportError::MissingMassItem { kind: label }),
            (Some(_), Some(_)) => Err(AceExportError::OutOfRange {
                quantity: "mass items of one kind",
                value: self.model.mass_items.len() as f64,
                expected: "one each of empty, payload and fuel",
            }),
        }
    }

    /// The model's inertia of `item`, or the estimate.
    fn empty_inertia(
        &self,
        item: &MassItem,
        mass_kg: f64,
        span_m: f64,
    ) -> Result<DMat3, AceExportError> {
        let Some(inertia) = &item.inertia else {
            return Ok(estimated_inertia(mass_kg, span_m));
        };
        let component = |binding| self.binding(binding, Unit::Inertia);
        let (xx, yy, zz) = (
            component(&inertia.xx)?,
            component(&inertia.yy)?,
            component(&inertia.zz)?,
        );
        let (xy, xz, yz) = (
            component(&inertia.xy)?,
            component(&inertia.xz)?,
            component(&inertia.yz)?,
        );
        // Model `x` aft and `z` up become forward and down, so the products
        // that touch exactly one of those two axes change sign.
        Ok(DMat3::from_cols_array(&[
            xx, -xy, xz, -xy, yy, -yz, xz, -yz, zz,
        ]))
    }
}

fn estimated_inertia(mass_kg: f64, span_m: f64) -> DMat3 {
    let length_m = span_m * LENGTH_OVER_SPAN;
    let DVec3 { x, y, z } = RADIUS_OF_GYRATION;
    DMat3::from_diagonal(DVec3::new(
        mass_kg * (span_m * x / 2.0).powi(2),
        mass_kg * (length_m * y / 2.0).powi(2),
        mass_kg * ((span_m + length_m) * z / 4.0).powi(2),
    ))
}

pub(crate) fn require(
    quantity: &'static str,
    value: f64,
    valid: bool,
    expected: &'static str,
) -> Result<f64, AceExportError> {
    if value.is_finite() && valid {
        return Ok(value);
    }
    Err(AceExportError::OutOfRange {
        quantity,
        value,
        expected,
    })
}

pub(crate) fn positive(quantity: &'static str, value: f64) -> Result<f64, AceExportError> {
    require(quantity, value, value > 0.0, "positive")
}

impl FixedWingDesign {
    /// Validates `model` and reads what flying it takes, refusing what would
    /// make a nonsensical aircraft rather than flying it.
    pub fn from_model(model: &VehicleModel) -> Result<Self, AceExportError> {
        model.validate().map_err(AceExportError::InvalidModel)?;
        if model.domain != FIXED_WING {
            return Err(AceExportError::UnsupportedDomain {
                domain: model.domain.clone(),
            });
        }
        let values = Values::of(model);
        let read = |name, unit| values.parameter(name, unit);
        let engines = read("engine.count", Unit::Ratio)?;
        require(
            "engine.count",
            engines,
            engines == 1.0,
            "exactly one engine",
        )?;
        let empty_item = values.single_item(MassKind::Empty, "empty")?;
        let empty = values.mass_station(empty_item)?;
        let payload = values.mass_station(values.single_item(MassKind::Payload, "payload")?)?;
        let fuel_item = values.single_item(MassKind::Fuel, "fuel")?;
        let fuel = values.mass_station(fuel_item)?;
        let span = positive("wing.span", read("wing.span", Unit::Meter)?)?;
        let oswald = read("aero.oswald", Unit::Ratio)?;
        let cd0 = positive("aero.cd0", read("aero.cd0", Unit::Ratio)?)?;
        let cl_max = positive("aero.cl_max", read("aero.cl_max", Unit::Ratio)?)?;
        let design = Self {
            id: model.id.clone(),
            name: model.name.clone(),
            wing_area_m2: positive("wing.area", read("wing.area", Unit::SquareMeter)?)?,
            wing_span_m: span,
            cd0,
            oswald: require(
                "aero.oswald",
                oswald,
                oswald > 0.0 && oswald <= 1.0,
                "in (0, 1]",
            )?,
            cl_max,
            shaft_power_w: positive(
                "engine.shaft_power",
                read("engine.shaft_power", Unit::Watt)?,
            )?,
            powerplant: catalog::powerplant(&values)?,
            propeller: catalog::propeller(&values)?,
            flaps: catalog::flaps(&values, cl_max, cd0)?,
            cruise_speed_m_s: positive(
                "phase.cruise.speed",
                read("phase.cruise.speed", Unit::MeterPerSecond)?,
            )?,
            cruise_altitude_m: read("phase.cruise.altitude", Unit::Meter)?,
            empty,
            payload,
            fuel,
            fuel_capacity_kg: match &fuel_item.capacity {
                Some(capacity) => values.binding(capacity, Unit::Kilogram)?,
                None => fuel.mass_kg,
            },
            empty_inertia_kg_m2: values.empty_inertia(empty_item, empty.mass_kg, span)?,
        };
        design.validate_masses(&values)?;
        Ok(design)
    }

    fn validate_masses(&self, values: &Values<'_>) -> Result<(), AceExportError> {
        positive("weight.oew", self.empty.mass_kg)?;
        let (fuel, payload) = (self.fuel.mass_kg, self.payload.mass_kg);
        require("fuel.loaded", fuel, fuel >= 0.0, "not negative")?;
        require("payload.mass", payload, payload >= 0.0, "not negative")?;
        let takeoff = self.loaded_mass_kg();
        let limit = values.parameter("weight.mtow", Unit::Kilogram)?;
        require(
            "loaded mass",
            takeoff,
            takeoff <= limit,
            "at most weight.mtow",
        )?;
        let capacity = self.fuel_capacity_kg;
        require(
            "fuel capacity",
            capacity,
            capacity >= fuel,
            "at least the fuel loaded",
        )?;
        Ok(())
    }

    /// Empty, payload and fuel together, kg.
    pub fn loaded_mass_kg(&self) -> f64 {
        self.empty.mass_kg + self.payload.mass_kg + self.fuel.mass_kg
    }

    /// Centre of gravity as loaded, in the model's axes, metres.
    pub fn loaded_cg_station_m(&self) -> DVec3 {
        let moment = [self.empty, self.payload, self.fuel]
            .iter()
            .map(|item| item.station_m * item.mass_kg)
            .sum::<DVec3>();
        moment / self.loaded_mass_kg()
    }

    /// Mean aerodynamic chord, taken as the rectangular wing's, metres.
    pub fn mean_chord_m(&self) -> f64 {
        self.wing_area_m2 / self.wing_span_m
    }

    /// Span²/area.
    pub fn aspect_ratio(&self) -> f64 {
        self.wing_span_m * self.wing_span_m / self.wing_area_m2
    }

    /// Whole-wing lift-curve slope, per radian (Helmbold).
    pub fn lift_slope_per_rad(&self) -> f64 {
        let aspect = self.aspect_ratio();
        SECTION_LIFT_SLOPE * aspect / (2.0 + (4.0 + aspect * aspect).sqrt())
    }

    /// Zero-lift angle of attack, radians.
    pub fn zero_lift_alpha_rad(&self) -> f64 {
        ZERO_LIFT_ALPHA_RAD
    }

    /// `cl²` coefficient of the induced drag.
    pub fn induced_drag_factor(&self) -> f64 {
        1.0 / (PI * self.oswald * self.aspect_ratio())
    }

    /// ISA density at the cruise altitude, kg/m³.
    pub fn cruise_density_kg_m3(&self) -> Result<f64, AceExportError> {
        atmosphere::isa_density_kg_m3(self.cruise_altitude_m).ok_or(AceExportError::OutOfRange {
            quantity: "phase.cruise.altitude",
            value: self.cruise_altitude_m,
            expected: "inside the standard atmosphere's lower two layers",
        })
    }

    /// The angle of attack that carries the loaded aircraft at the cruise
    /// point, radians.
    pub fn cruise_alpha_rad(&self) -> Result<f64, AceExportError> {
        let density = self.cruise_density_kg_m3()?;
        let dynamic_pressure = 0.5 * density * self.cruise_speed_m_s * self.cruise_speed_m_s;
        let weight_n = self.loaded_mass_kg() * STANDARD_GRAVITY_M_S2;
        let cruise_cl = weight_n / (dynamic_pressure * self.wing_area_m2);
        Ok(cruise_cl / self.lift_slope_per_rad() + ZERO_LIFT_ALPHA_RAD)
    }
}
