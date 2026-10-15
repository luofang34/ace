use serde::{Deserialize, Serialize};

use crate::{ModelError, ModelResult};

/// Explicit interface units; calculations retain physical dimensions in SI.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Unit {
    /// Dimensionless number.
    #[serde(rename = "1", alias = "dimensionless")]
    Ratio,
    /// Kilograms.
    #[serde(rename = "kg")]
    Kilogram,
    /// Pounds of mass.
    #[serde(rename = "lb")]
    Pound,
    /// Metres.
    #[serde(rename = "m")]
    Meter,
    /// Feet.
    #[serde(rename = "ft")]
    Foot,
    /// Nautical miles.
    #[serde(rename = "nmi")]
    NauticalMile,
    /// Square metres.
    #[serde(rename = "m^2")]
    SquareMeter,
    /// Cubic metres.
    #[serde(rename = "m^3")]
    CubicMeter,
    /// Seconds.
    #[serde(rename = "s")]
    Second,
    /// Hours.
    #[serde(rename = "h")]
    Hour,
    /// Metres per second.
    #[serde(rename = "m/s")]
    MeterPerSecond,
    /// Knots.
    #[serde(rename = "kt")]
    Knot,
    /// Newtons.
    #[serde(rename = "N")]
    Newton,
    /// Watts.
    #[serde(rename = "W")]
    Watt,
    /// Radians.
    #[serde(rename = "rad")]
    Radian,
    /// Degrees.
    #[serde(rename = "deg")]
    Degree,
    /// Kilograms per second.
    #[serde(rename = "kg/s")]
    KilogramPerSecond,
    /// Kilograms per cubic metre.
    #[serde(rename = "kg/m^3")]
    KilogramPerCubicMeter,
    /// Kilogram square metres.
    #[serde(rename = "kg*m^2")]
    Inertia,
    /// US dollars in the project's declared economic basis.
    #[serde(rename = "USD")]
    Dollar,
    /// US dollars per metre.
    #[serde(rename = "USD/m")]
    DollarPerMeter,
    /// US dollars per nautical mile.
    #[serde(rename = "USD/nmi")]
    DollarPerNauticalMile,
    /// US dollars per second.
    #[serde(rename = "USD/s")]
    DollarPerSecond,
    /// US dollars per flight or ground hour in the declared economic basis.
    #[serde(rename = "USD/h")]
    DollarPerHour,
    /// US dollars per kilogram of consumable.
    #[serde(rename = "USD/kg")]
    DollarPerKilogram,
    /// Thrust-specific fuel consumption in kilograms per newton second.
    #[serde(rename = "kg/(N*s)")]
    KilogramPerNewtonSecond,
}

impl Unit {
    fn definition(self) -> (f64, [i16; 5]) {
        match self {
            Self::Ratio => (1.0, [0, 0, 0, 0, 0]),
            Self::Kilogram => (1.0, [1, 0, 0, 0, 0]),
            Self::Pound => (0.453_592_37, [1, 0, 0, 0, 0]),
            Self::Meter => (1.0, [0, 1, 0, 0, 0]),
            Self::Foot => (0.3048, [0, 1, 0, 0, 0]),
            Self::NauticalMile => (1852.0, [0, 1, 0, 0, 0]),
            Self::SquareMeter => (1.0, [0, 2, 0, 0, 0]),
            Self::CubicMeter => (1.0, [0, 3, 0, 0, 0]),
            Self::Second => (1.0, [0, 0, 1, 0, 0]),
            Self::Hour => (3600.0, [0, 0, 1, 0, 0]),
            Self::MeterPerSecond => (1.0, [0, 1, -1, 0, 0]),
            Self::Knot => (1852.0 / 3600.0, [0, 1, -1, 0, 0]),
            Self::Newton => (1.0, [1, 1, -2, 0, 0]),
            Self::Watt => (1.0, [1, 2, -3, 0, 0]),
            Self::Radian => (1.0, [0, 0, 0, 1, 0]),
            Self::Degree => (std::f64::consts::PI / 180.0, [0, 0, 0, 1, 0]),
            Self::KilogramPerSecond => (1.0, [1, 0, -1, 0, 0]),
            Self::KilogramPerCubicMeter => (1.0, [1, -3, 0, 0, 0]),
            Self::Inertia => (1.0, [1, 2, 0, 0, 0]),
            Self::Dollar => (1.0, [0, 0, 0, 0, 1]),
            Self::DollarPerMeter => (1.0, [0, -1, 0, 0, 1]),
            Self::DollarPerNauticalMile => (1.0 / 1852.0, [0, -1, 0, 0, 1]),
            Self::DollarPerSecond => (1.0, [0, 0, -1, 0, 1]),
            Self::DollarPerHour => (1.0 / 3600.0, [0, 0, -1, 0, 1]),
            Self::DollarPerKilogram => (1.0, [-1, 0, 0, 0, 1]),
            Self::KilogramPerNewtonSecond => (1.0, [0, -1, 1, 0, 0]),
        }
    }

    /// Whether two units measure the same physical dimensions.
    pub fn compatible(self, other: Self) -> bool {
        self.definition().1 == other.definition().1
    }
}

/// Finite scalar with explicit units at storage and API boundaries.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Quantity {
    /// Magnitude in the declared unit.
    pub value: f64,
    /// Physical unit, never inferred from a parameter name.
    pub unit: Unit,
}

impl Quantity {
    /// Validate and construct a scalar.
    pub fn new(value: f64, unit: Unit) -> ModelResult<Self> {
        if !value.is_finite() {
            return Err(ModelError::invalid("quantity", "value must be finite"));
        }
        Ok(Self { value, unit })
    }

    /// Convert through the SI representation, rejecting mismatched dimensions.
    pub fn to(self, unit: Unit) -> ModelResult<Self> {
        self.dimensional()?.to(unit)
    }

    /// Normalize into dimensional arithmetic.
    pub fn dimensional(self) -> ModelResult<DimensionalValue> {
        let (factor, dimensions) = self.unit.definition();
        DimensionalValue::new(self.value * factor, dimensions)
    }
}

/// Intermediate SI scalar whose dimensions may be compound.
#[derive(Debug, Clone, Copy)]
pub struct DimensionalValue {
    value: f64,
    dimensions: [i16; 5],
}

impl DimensionalValue {
    fn new(value: f64, dimensions: [i16; 5]) -> ModelResult<Self> {
        if !value.is_finite() {
            return Err(ModelError::invalid(
                "expression",
                "non-finite arithmetic result",
            ));
        }
        Ok(Self { value, dimensions })
    }

    /// Convert an arithmetic result to the parameter's declared unit.
    pub fn to(self, unit: Unit) -> ModelResult<Quantity> {
        let (factor, dimensions) = unit.definition();
        if self.dimensions != dimensions {
            return Err(ModelError::Dimensions(format!(
                "{:?} cannot convert to {unit:?}",
                self.dimensions
            )));
        }
        Quantity::new(self.value / factor, unit)
    }

    /// Add or subtract compatible dimensional values.
    pub fn add(self, other: Self, subtract: bool) -> ModelResult<Self> {
        if self.dimensions != other.dimensions {
            return Err(ModelError::Dimensions(
                "addition requires equal dimensions".into(),
            ));
        }
        Self::new(
            self.value + if subtract { -other.value } else { other.value },
            self.dimensions,
        )
    }

    /// Multiply or divide, carrying exponents and detecting singular results.
    pub fn multiply(self, other: Self, divide: bool) -> ModelResult<Self> {
        let mut dimensions = self.dimensions;
        for (exponent, other) in dimensions.iter_mut().zip(other.dimensions) {
            *exponent = if divide {
                exponent.checked_sub(other)
            } else {
                exponent.checked_add(other)
            }
            .ok_or_else(|| ModelError::Dimensions("dimension exponent overflow".into()))?;
        }
        let value = if divide {
            self.value / other.value
        } else {
            self.value * other.value
        };
        Self::new(value, dimensions)
    }

    /// Raise a value to a bounded integer power.
    pub fn power(self, power: i8) -> ModelResult<Self> {
        if !(-16..=16).contains(&power) {
            return Err(ModelError::invalid(
                "expression.power",
                "power must be between -16 and 16",
            ));
        }
        let mut dimensions = self.dimensions;
        for exponent in &mut dimensions {
            *exponent = exponent
                .checked_mul(i16::from(power))
                .ok_or_else(|| ModelError::Dimensions("dimension exponent overflow".into()))?;
        }
        Self::new(self.value.powi(i32::from(power)), dimensions)
    }
}

#[cfg(test)]
mod tests;
