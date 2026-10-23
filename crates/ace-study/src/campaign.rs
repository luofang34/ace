use crate::{ProbabilityStatus, StudyError, StudyResult, VehicleCapability};
use ace_model::{Quantity, Unit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod simulation;

/// Identity of the campaign model; changing it changes every record.
const ALGORITHM: &str = "persistent-station campaign/1; splitmix64 streams: shared environment stream per replication, per-fleet sortie stream; daily weather no-fly draw; sorties flown up to whole-vehicle cycle capacity; per-sortie loss after its station time and comms-loss degradation; replacements arrive after the lead time; success interval over replication means";

/// Required persistent presence over an area.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Demand {
    /// Simultaneous stations to hold.
    pub stations: u32,
    /// One-way distance from base to station.
    pub transit_distance: Quantity,
    /// Required station time per station per day, at most 24 h.
    pub daily_coverage: Quantity,
    /// Campaign length in days.
    pub days: u32,
}

/// Stochastic operating conditions shared by every fleet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    /// Probability that weather prevents all flying on a day.
    pub no_fly_probability: f64,
    /// Probability that a sortie loses its command link.
    pub comms_loss_probability: f64,
    /// Fraction of station time that remains useful after a link loss.
    pub comms_loss_effectiveness: f64,
}

/// One candidate force structure built around a vehicle capability record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Fleet {
    /// Vehicle capability, traceable to its synthesis evidence.
    pub capability: VehicleCapability,
    /// Vehicles available on day one.
    pub initial_units: u32,
    /// Delay between a loss and its replacement's arrival.
    pub replacement_lead_time: Quantity,
    /// Replacements that may be bought during the campaign.
    pub max_replacements: u32,
    /// Operators staffed per held station.
    pub operators_per_station: f64,
    /// Burdened operator cost per staffed hour.
    pub operator_cost_per_hour: Quantity,
}

/// Mission-level comparison of force structures under a shared scenario distribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Campaign {
    /// Schema identifier; must be 1.
    pub schema_version: u32,
    /// Stable campaign name.
    pub name: String,
    /// Required presence.
    pub demand: Demand,
    /// Operating conditions.
    pub environment: Environment,
    /// Candidate force structures.
    pub fleets: BTreeMap<String, Fleet>,
    /// Fraction of required station time that makes a day successful.
    pub daily_success_fraction: f64,
    /// Minimum fraction of successful days; a hard threshold.
    pub required_success_rate: f64,
    /// Independent replications, from 2 through 10000.
    pub replications: u32,
    /// Seed of every random stream.
    pub seed: u64,
}

/// Mean campaign cost per replication.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CampaignCost {
    /// Initial vehicles and replacements at their unit cost.
    pub acquisition_usd: f64,
    /// Flight hours at the vehicle's operating cost.
    pub flight_usd: f64,
    /// Staffed operator hours.
    pub operators_usd: f64,
    /// Sum of the above.
    pub total_usd: f64,
}

/// Outcome of one fleet across all replications.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FleetOutcome {
    /// Capability record used.
    pub capability: String,
    /// Whether the capability rests on approximate evidence; outcomes always do.
    pub capability_approximate: bool,
    /// Station time per sortie after transit; nonpositive means the vehicle cannot hold a station.
    pub station_time_per_sortie_h: f64,
    /// Mean fraction of successful days.
    pub success_rate: f64,
    /// 95% lower bound over replication means.
    pub lower: f64,
    /// 95% upper bound over replication means.
    pub upper: f64,
    /// Comparison with the required success rate.
    pub status: ProbabilityStatus,
    /// Mean fraction of required station time delivered.
    pub delivered_fraction: f64,
    /// Mean vehicles lost per replication.
    pub losses: f64,
    /// Mean replacements bought per replication.
    pub replacements: f64,
    /// Mean days with no vehicle available.
    pub empty_days: f64,
    /// Mean cost per replication.
    pub cost: CampaignCost,
    /// Delivered station hours per million dollars; comparable only among passing fleets.
    pub station_hours_per_million_usd: f64,
}

/// Content-addressed campaign evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignRecord {
    /// SHA-256 of this record with this field empty.
    pub id: String,
    /// Campaign model identity.
    pub algorithm: String,
    /// Build that executed the campaign.
    pub build: ace_engine::BuildIdentity,
    /// Exact campaign definition.
    pub campaign: Campaign,
    /// Outcome per fleet.
    pub outcomes: BTreeMap<String, FleetOutcome>,
    /// Passing fleet with the most delivered station time per dollar; none when no fleet passes.
    pub preferred: Option<String>,
}

impl Campaign {
    /// Check ranges and every embedded capability record.
    pub fn validate(&self) -> StudyResult<()> {
        let environment = &self.environment;
        let probabilities = [
            environment.no_fly_probability,
            environment.comms_loss_probability,
            environment.comms_loss_effectiveness,
            self.daily_success_fraction,
            self.required_success_rate,
        ];
        let coverage = self.demand.daily_coverage.to(Unit::Hour)?.value;
        if self.schema_version != 1
            || self.name.trim().is_empty()
            || self.fleets.is_empty()
            || self.demand.stations == 0
            || !(1..=3650).contains(&self.demand.days)
            || !(2..=10_000).contains(&self.replications)
            || !(coverage > 0.0 && coverage <= 24.0)
            || self.demand.transit_distance.to(Unit::Meter)?.value < 0.0
            || probabilities
                .iter()
                .any(|value| !(0.0..=1.0).contains(value))
        {
            return Err(StudyError::invalid(
                &self.name,
                "campaign needs schema 1, stations, 1-3650 days, 2-10000 replications, coverage in (0, 24] h and probabilities in [0, 1]",
            ));
        }
        for (id, fleet) in &self.fleets {
            fleet.capability.verify()?;
            if fleet.initial_units == 0
                || fleet.replacement_lead_time.to(Unit::Second)?.value < 0.0
                || fleet.operator_cost_per_hour.to(Unit::DollarPerHour)?.value < 0.0
                || !(fleet.operators_per_station >= 0.0 && fleet.operators_per_station.is_finite())
            {
                return Err(StudyError::invalid(
                    id,
                    "fleet needs vehicles, a nonnegative lead time and nonnegative staffing",
                ));
            }
        }
        Ok(())
    }

    /// Simulate every fleet against the same environment draws.
    pub fn run(&self) -> StudyResult<CampaignRecord> {
        self.validate()?;
        let outcomes = self
            .fleets
            .iter()
            .map(|(id, fleet)| Ok((id.clone(), simulation::fleet(self, id, fleet)?)))
            .collect::<StudyResult<BTreeMap<_, _>>>()?;
        let preferred = outcomes
            .iter()
            .filter(|(_, outcome)| outcome.status == ProbabilityStatus::Pass)
            .max_by(|left, right| {
                left.1
                    .station_hours_per_million_usd
                    .total_cmp(&right.1.station_hours_per_million_usd)
            })
            .map(|(id, _)| id.clone());
        let mut record = CampaignRecord {
            id: String::new(),
            algorithm: ALGORITHM.into(),
            build: ace_engine::implementation_identity(),
            campaign: self.clone(),
            outcomes,
            preferred,
        };
        record.id = crate::record::digest(&record)?;
        Ok(record)
    }
}

impl CampaignRecord {
    /// Detect changed scenarios, capabilities or outcomes after serialization.
    pub fn verify_integrity(&self) -> StudyResult<()> {
        let mut copy = self.clone();
        copy.id.clear();
        if crate::record::digest(&copy)? != self.id {
            return Err(StudyError::invalid(
                &self.campaign.name,
                "campaign record digest does not match its data",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
