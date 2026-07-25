use serde::{Deserialize, Serialize};

pub(crate) const TABLE_PROPULSION_MODEL_ID: &str = "propulsion.table_deck";

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PropulsionMode {
    Takeoff,
    Climb,
    Cruise,
    Economy,
}

impl PropulsionMode {
    pub(crate) const ALL: [Self; 4] = [Self::Takeoff, Self::Climb, Self::Cruise, Self::Economy];

    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            Self::Takeoff => "takeoff",
            Self::Climb => "climb",
            Self::Cruise => "cruise",
            Self::Economy => "economy",
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub(crate) struct TablePropulsionDeck {
    pub(crate) mach_axis: Vec<f64>,
    pub(crate) altitude_axis_m: Vec<f64>,
    pub(crate) modes: Vec<TablePropulsionMode>,
}

impl TablePropulsionDeck {
    pub(crate) fn mode(&self, mode: PropulsionMode) -> Option<&TablePropulsionMode> {
        self.modes.iter().find(|item| item.mode == mode)
    }

    pub(crate) fn interpolate(
        &self,
        mode: PropulsionMode,
        altitude_m: f64,
        mach: f64,
    ) -> Option<TableInterpolation> {
        let mode = self.mode(mode)?;
        let altitude = bracket(&self.altitude_axis_m, altitude_m)?;
        let mach = bracket(&self.mach_axis, mach)?;
        Some(TableInterpolation {
            thrust_per_engine_n: interpolate_matrix(&mode.thrust_n, altitude, mach)?,
            fuel: match &mode.fuel {
                TableFuelSchedule::TsfcKgNHr(values) => {
                    TableFuelValue::TsfcKgNHr(interpolate_matrix(values, altitude, mach)?)
                }
                TableFuelSchedule::SpecificImpulseS(values) => {
                    TableFuelValue::SpecificImpulseS(interpolate_matrix(values, altitude, mach)?)
                }
            },
            altitude_extrapolated: altitude.extrapolated,
            mach_extrapolated: mach.extrapolated,
        })
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub(crate) struct TablePropulsionMode {
    pub(crate) mode: PropulsionMode,
    pub(crate) thrust_n: Vec<Vec<f64>>,
    pub(crate) fuel: TableFuelSchedule,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(tag = "basis", content = "values", rename_all = "snake_case")]
pub(crate) enum TableFuelSchedule {
    TsfcKgNHr(Vec<Vec<f64>>),
    SpecificImpulseS(Vec<Vec<f64>>),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TableInterpolation {
    pub(crate) thrust_per_engine_n: f64,
    pub(crate) fuel: TableFuelValue,
    pub(crate) altitude_extrapolated: bool,
    pub(crate) mach_extrapolated: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TableFuelValue {
    TsfcKgNHr(f64),
    SpecificImpulseS(f64),
}

impl TableFuelValue {
    pub(crate) fn flow_for_thrust(self, thrust_n: f64) -> f64 {
        match self {
            Self::TsfcKgNHr(value) => value * thrust_n / 3600.0,
            Self::SpecificImpulseS(value) => {
                thrust_n / (value * crate::domain::quantity::GRAVITY_M_S2)
            }
        }
    }

    pub(crate) fn value(self) -> f64 {
        match self {
            Self::TsfcKgNHr(value) | Self::SpecificImpulseS(value) => value,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AxisBracket {
    lower: usize,
    upper: usize,
    fraction: f64,
    extrapolated: bool,
}

fn bracket(axis: &[f64], value: f64) -> Option<AxisBracket> {
    let first = *axis.first()?;
    let final_index = axis.len().checked_sub(1)?;
    let last = *axis.get(final_index)?;
    if final_index == 0 {
        return None;
    }
    let (lower, upper, extrapolated) = if value < first {
        (0, 1, true)
    } else if value > last {
        (final_index - 1, final_index, true)
    } else {
        interior_bracket(axis, value, final_index)
    };
    Some(AxisBracket {
        lower,
        upper,
        fraction: (value - axis.get(lower)?) / (axis.get(upper)? - axis.get(lower)?),
        extrapolated,
    })
}

fn interior_bracket(axis: &[f64], value: f64, final_index: usize) -> (usize, usize, bool) {
    let upper = axis.partition_point(|axis_value| *axis_value < value);
    if upper == 0 {
        (0, 1, false)
    } else if upper >= axis.len() {
        (final_index - 1, final_index, false)
    } else {
        (upper - 1, upper, false)
    }
}

fn interpolate_matrix(
    values: &[Vec<f64>],
    altitude: AxisBracket,
    mach: AxisBracket,
) -> Option<f64> {
    let low = linear(
        cell(values, altitude.lower, mach.lower)?,
        cell(values, altitude.lower, mach.upper)?,
        mach.fraction,
    );
    let high = linear(
        cell(values, altitude.upper, mach.lower)?,
        cell(values, altitude.upper, mach.upper)?,
        mach.fraction,
    );
    Some(linear(low, high, altitude.fraction))
}

fn cell(values: &[Vec<f64>], row: usize, column: usize) -> Option<f64> {
    values.get(row)?.get(column).copied()
}

fn linear(lower: f64, upper: f64, fraction: f64) -> f64 {
    lower + fraction * (upper - lower)
}
