use serde::{Deserialize, Serialize};
use uom::si::angle::{degree, radian};
use uom::si::area::{square_foot, square_meter};
use uom::si::f64::{Angle, Area, Length, Mass, Power, Time, Velocity};
use uom::si::length::{foot, meter, nautical_mile};
use uom::si::mass::{kilogram, pound};
use uom::si::power::{horsepower, kilowatt, watt};
use uom::si::time::{hour, minute, second};
use uom::si::velocity::{knot, meter_per_second, mile_per_hour};

use crate::domain::diagnostic::{AexError, AexResult};

pub(crate) const GRAVITY_M_S2: f64 = 9.806_65;
pub(crate) const NAUTICAL_MILE_M: f64 = 1852.0;
pub(crate) const KNOT_M_S: f64 = NAUTICAL_MILE_M / 3600.0;
pub(crate) const FOOT_M: f64 = 0.3048;
pub(crate) const POUND_FORCE_N: f64 = 4.448_221_615_260_5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dimension {
    Mass,
    Length,
    Area,
    Speed,
    Time,
    Power,
    Force,
    Angle,
    RotationSpeed,
}

impl Dimension {
    fn target(self) -> &'static str {
        match self {
            Self::Mass => "kg",
            Self::Length => "m",
            Self::Area => "m^2",
            Self::Speed => "m/s",
            Self::Time => "s",
            Self::Power => "W",
            Self::Force => "N",
            Self::Angle => "rad",
            Self::RotationSpeed => "rad/s",
        }
    }
}

pub(crate) fn parse_quantity(raw: &str, dimension: Dimension) -> AexResult<f64> {
    let (value, unit) = split_quantity(raw, dimension.target())?;
    let normalized = match dimension {
        Dimension::Mass => parse_mass(value, unit),
        Dimension::Length => parse_length(value, unit),
        Dimension::Area => parse_area(value, unit),
        Dimension::Speed => parse_speed(value, unit),
        Dimension::Time => parse_time(value, unit),
        Dimension::Power => parse_power(value, unit),
        Dimension::Force => parse_force(value, unit),
        Dimension::Angle => parse_angle(value, unit),
        Dimension::RotationSpeed => parse_rotation_speed(value, unit),
    };
    normalized.ok_or_else(|| AexError::Quantity {
        value: raw.to_owned(),
        target: dimension.target(),
        reason: format!("unit {unit:?} is not compatible"),
    })
}

fn split_quantity<'a>(raw: &'a str, target: &'static str) -> AexResult<(f64, &'a str)> {
    let split_at = raw
        .find(char::is_whitespace)
        .ok_or_else(|| AexError::Quantity {
            value: raw.to_owned(),
            target,
            reason: "physical values require an explicit unit".to_owned(),
        })?;
    let (number, remainder) = raw.split_at(split_at);
    let value = number.parse::<f64>().map_err(|source| AexError::Quantity {
        value: raw.to_owned(),
        target,
        reason: source.to_string(),
    })?;
    let unit = remainder.trim();
    if !value.is_finite() {
        return Err(AexError::Quantity {
            value: raw.to_owned(),
            target,
            reason: "value must be finite".to_owned(),
        });
    }
    Ok((value, unit))
}

fn parse_mass(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "kg" => Some(Mass::new::<kilogram>(value).get::<kilogram>()),
        "lb" | "lbs" => Some(Mass::new::<pound>(value).get::<kilogram>()),
        "t" | "tonne" | "tonnes" => Some(value * 1000.0),
        _ => None,
    }
}

fn parse_length(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "m" => Some(Length::new::<meter>(value).get::<meter>()),
        "ft" => Some(Length::new::<foot>(value).get::<meter>()),
        "nmi" | "NM" => Some(Length::new::<nautical_mile>(value).get::<meter>()),
        "km" => Some(value * 1000.0),
        _ => None,
    }
}

fn parse_area(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "m^2" | "m2" => Some(Area::new::<square_meter>(value).get::<square_meter>()),
        "ft^2" | "ft2" => Some(Area::new::<square_foot>(value).get::<square_meter>()),
        _ => None,
    }
}

fn parse_speed(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "m/s" => Some(Velocity::new::<meter_per_second>(value).get::<meter_per_second>()),
        "kt" | "kts" | "knot" | "knots" => {
            Some(Velocity::new::<knot>(value).get::<meter_per_second>())
        }
        "mph" => Some(Velocity::new::<mile_per_hour>(value).get::<meter_per_second>()),
        _ => None,
    }
}

fn parse_time(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "s" | "sec" => Some(Time::new::<second>(value).get::<second>()),
        "min" => Some(Time::new::<minute>(value).get::<second>()),
        "h" | "hr" | "hour" => Some(Time::new::<hour>(value).get::<second>()),
        _ => None,
    }
}

fn parse_power(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "W" => Some(Power::new::<watt>(value).get::<watt>()),
        "kW" => Some(Power::new::<kilowatt>(value).get::<watt>()),
        "hp" => Some(Power::new::<horsepower>(value).get::<watt>()),
        _ => None,
    }
}

fn parse_force(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "N" => Some(value),
        "kN" => Some(value * 1000.0),
        "lbf" => Some(value * POUND_FORCE_N),
        _ => None,
    }
}

fn parse_angle(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "rad" => Some(Angle::new::<radian>(value).get::<radian>()),
        "deg" | "degree" => Some(Angle::new::<degree>(value).get::<radian>()),
        _ => None,
    }
}

fn parse_rotation_speed(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "rad/s" => Some(value),
        "rpm" => Some(value * std::f64::consts::TAU / 60.0),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct QuantityOutput {
    pub(crate) value: f64,
    pub(crate) unit: String,
    pub(crate) display_value: f64,
    pub(crate) display_unit: String,
}

impl QuantityOutput {
    pub(crate) fn range(value_m: f64) -> Self {
        Self {
            value: value_m,
            unit: "m".to_owned(),
            display_value: value_m / NAUTICAL_MILE_M,
            display_unit: "nmi".to_owned(),
        }
    }

    pub(crate) fn si(value: f64, unit: &str) -> Self {
        Self {
            value,
            unit: unit.to_owned(),
            display_value: value,
            display_unit: unit.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests;
