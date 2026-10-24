use super::{Campaign, CampaignCost, Fleet, FleetOutcome};
use crate::uncertainty::sampling::SplitMix64;
use crate::{ProbabilityStatus, StudyResult};
use ace_model::Unit;
use sha2::{Digest, Sha256};

const DAY_S: f64 = 86_400.0;
const Z95: f64 = 1.959_963_984_540_054;

/// Fleet-independent quantities of one force structure, in seconds and dollars.
struct Plan {
    station_s: f64,
    transit_s: f64,
    cycle_s: f64,
    required_s: f64,
    lead_days: u32,
    loss: f64,
}

/// Per-replication totals.
#[derive(Default)]
struct Totals {
    successful_days: u32,
    delivered_s: f64,
    flight_s: f64,
    losses: u32,
    bought: u32,
    empty_days: u32,
}

/// Mutable fleet state during one replication.
struct State {
    units: u32,
    credit: f64,
    arrivals: Vec<u32>,
    totals: Totals,
}

pub(super) fn fleet(campaign: &Campaign, id: &str, fleet: &Fleet) -> StudyResult<FleetOutcome> {
    let values = &fleet.capability.values;
    let endurance = values.endurance.to(Unit::Second)?.value;
    let transit_s = 2.0 * campaign.demand.transit_distance.to(Unit::Meter)?.value
        / values.cruise_speed.to(Unit::MeterPerSecond)?.value;
    let coverage_s = campaign.demand.daily_coverage.to(Unit::Second)?.value;
    let plan = Plan {
        station_s: endurance - transit_s,
        transit_s,
        cycle_s: endurance + values.turnaround.to(Unit::Second)?.value,
        required_s: f64::from(campaign.demand.stations) * coverage_s,
        lead_days: (fleet.replacement_lead_time.to(Unit::Second)?.value / DAY_S)
            .ceil()
            .max(1.0) as u32,
        loss: values.loss_per_sortie.to(Unit::Ratio)?.value,
    };
    let digest = Sha256::digest(id.as_bytes());
    let mut salt = [0_u8; 8];
    salt.copy_from_slice(digest.get(..8).unwrap_or(&[0; 8]));
    let mut environment = SplitMix64::new(campaign.seed);
    let mut sorties = SplitMix64::new(campaign.seed ^ u64::from_le_bytes(salt));
    let replications: Vec<Totals> = (0..campaign.replications)
        .map(|_| replicate(campaign, fleet, &plan, &mut environment, &mut sorties))
        .collect();
    outcome(campaign, fleet, &plan, &replications, coverage_s)
}

fn replicate(
    campaign: &Campaign,
    fleet: &Fleet,
    plan: &Plan,
    environment: &mut SplitMix64,
    sorties: &mut SplitMix64,
) -> Totals {
    let mut state = State {
        units: fleet.initial_units,
        credit: 0.0,
        arrivals: Vec::new(),
        totals: Totals::default(),
    };
    let per_unit = DAY_S / plan.cycle_s;
    for day in 0..campaign.demand.days {
        let arrived = state.arrivals.iter().filter(|due| **due == day).count() as u32;
        state.units += arrived;
        state.arrivals.retain(|due| *due != day);
        let no_fly = environment.unit() < campaign.environment.no_fly_probability;
        let units = f64::from(state.units);
        state.credit = (state.credit + units * per_unit).min(units * per_unit.max(1.0));
        if state.units == 0 {
            state.totals.empty_days += 1;
        }
        if no_fly || state.units == 0 || plan.station_s <= 0.0 {
            continue;
        }
        let delivered = fly_day(campaign, fleet, plan, &mut state, sorties, day);
        state.totals.delivered_s += delivered;
        if delivered >= campaign.daily_success_fraction * plan.required_s {
            state.totals.successful_days += 1;
        }
    }
    state.totals
}

fn fly_day(
    campaign: &Campaign,
    fleet: &Fleet,
    plan: &Plan,
    state: &mut State,
    sorties: &mut SplitMix64,
    day: u32,
) -> f64 {
    let environment = &campaign.environment;
    let mut remaining = plan.required_s;
    let mut delivered = 0.0;
    while remaining > 0.0 && state.credit >= 1.0 && state.units > 0 {
        state.credit -= 1.0;
        let station = plan.station_s.min(remaining);
        remaining -= station;
        delivered += if sorties.unit() < environment.comms_loss_probability {
            station * environment.comms_loss_effectiveness
        } else {
            station
        };
        state.totals.flight_s += plan.transit_s + station;
        if sorties.unit() < plan.loss {
            state.units -= 1;
            state.totals.losses += 1;
            state.credit = state.credit.min(f64::from(state.units));
            if state.totals.bought < fleet.max_replacements {
                state.totals.bought += 1;
                state.arrivals.push(day + plan.lead_days);
            }
        }
    }
    delivered
}

fn outcome(
    campaign: &Campaign,
    fleet: &Fleet,
    plan: &Plan,
    replications: &[Totals],
    coverage_s: f64,
) -> StudyResult<FleetOutcome> {
    let count = replications.len() as f64;
    let days = f64::from(campaign.demand.days);
    let mean = |value: &dyn Fn(&Totals) -> f64| replications.iter().map(value).sum::<f64>() / count;
    let rates: Vec<f64> = replications
        .iter()
        .map(|item| f64::from(item.successful_days) / days)
        .collect();
    let success_rate = rates.iter().sum::<f64>() / count;
    let spread = (rates
        .iter()
        .map(|rate| (rate - success_rate).powi(2))
        .sum::<f64>()
        / (count - 1.0))
        .sqrt();
    let half = Z95 * spread / count.sqrt();
    let (lower, upper) = (
        (success_rate - half).max(0.0),
        (success_rate + half).min(1.0),
    );
    let target = campaign.required_success_rate;
    let status = if lower >= target {
        ProbabilityStatus::Pass
    } else if upper < target {
        ProbabilityStatus::Fail
    } else {
        ProbabilityStatus::Indeterminate
    };
    let values = &fleet.capability.values;
    let unit_cost = values.unit_cost.to(Unit::Dollar)?.value;
    let hourly = values
        .operating_cost_per_hour
        .to(Unit::DollarPerHour)?
        .value;
    let operator_rate = fleet.operator_cost_per_hour.to(Unit::DollarPerHour)?.value;
    let acquisition = mean(&|item| f64::from(fleet.initial_units + item.bought) * unit_cost);
    let flight = mean(&|item| item.flight_s / 3600.0 * hourly);
    let operators = fleet.operators_per_station * f64::from(campaign.demand.stations) * coverage_s
        / 3600.0
        * days
        * operator_rate;
    let total = acquisition + flight + operators;
    let delivered_h = mean(&|item| item.delivered_s) / 3600.0;
    Ok(FleetOutcome {
        capability: fleet.capability.id.clone(),
        capability_approximate: fleet.capability.approximate,
        station_time_per_sortie_h: plan.station_s / 3600.0,
        success_rate,
        lower,
        upper,
        status,
        delivered_fraction: mean(&|item| item.delivered_s) / (plan.required_s * days),
        losses: mean(&|item| f64::from(item.losses)),
        replacements: mean(&|item| f64::from(item.bought)),
        empty_days: mean(&|item| f64::from(item.empty_days)),
        cost: CampaignCost {
            acquisition_usd: acquisition,
            flight_usd: flight,
            operators_usd: operators,
            total_usd: total,
        },
        station_hours_per_million_usd: if total > 0.0 {
            delivered_h / total * 1e6
        } else {
            0.0
        },
    })
}
