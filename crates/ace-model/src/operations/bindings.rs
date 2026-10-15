use super::{Airport, Operations};
use crate::{Binding, ModelError, ModelResult, Unit};

impl Operations {
    /// Complete dimensional dependency set, including all operating scenarios and rates.
    pub fn bindings(&self) -> Vec<(&Binding, Unit)> {
        use Unit::*;
        let mut values = vec![
            (&self.seats, Ratio),
            (&self.distance, Meter),
            (&self.load_factor, Ratio),
        ];
        values.extend(self.costs.iter().map(|cost| (&cost.amount, Dollar)));
        values.extend(
            self.span
                .iter()
                .chain(&self.required_runway)
                .map(|value| (value, Meter)),
        );
        values.extend(
            self.revenue_per_passenger
                .iter()
                .map(|value| (value, Dollar)),
        );
        airports(&mut values, &self.airports);
        if let Some(model) = &self.economics {
            let rates = &model.rates;
            values.extend([
                (&rates.fuel, DollarPerKilogram),
                (&rates.crew_flight_hour, DollarPerHour),
                (&rates.maintenance_flight_hour, DollarPerHour),
                (&rates.maintenance_cycle, Dollar),
                (&rates.turnaround_hour, DollarPerHour),
                (&rates.annual_ownership, Dollar),
                (&model.utilization.flight_time, Second),
                (&model.utilization.operating_time, Second),
            ]);
            for scenario in model.scenarios.values() {
                values.extend([
                    (&scenario.distance, Meter),
                    (&scenario.block_duration, Second),
                    (&scenario.block_fuel, Kilogram),
                    (&scenario.turnaround, Second),
                ]);
                values.extend(scenario.load_factor.iter().map(|value| (value, Ratio)));
                if let Some(endpoints) = &scenario.airports {
                    airports(&mut values, endpoints);
                }
            }
        }
        values
    }

    /// Enforce a declared common monetary basis without implicit currency/year conversion.
    pub fn validate_economics(&self) -> ModelResult<()> {
        if let Some(basis) = &self.basis
            && (basis.currency != "USD"
                || basis.price_year < 1900
                || basis.cost_scope.trim().is_empty())
        {
            return Err(ModelError::invalid(
                "operations.basis",
                "native costs require USD, an explicit price year and cost scope",
            ));
        }
        for cost in &self.costs {
            if cost.basis.is_some() && cost.basis != self.basis {
                return Err(ModelError::invalid(
                    &cost.id,
                    "cost currency, price year or scope differs from operating basis",
                ));
            }
        }
        if let Some(model) = &self.economics {
            if self.basis.is_none()
                || model.scenarios.is_empty()
                || !model.scenarios.contains_key(&model.selected)
            {
                return Err(ModelError::invalid(
                    "operations.economics",
                    "basis and a selected declared scenario are required",
                ));
            }
            for cost in &self.costs {
                if ["fuel", "crew", "maintenance", "ownership", "turnaround"]
                    .contains(&cost.category.as_str())
                {
                    return Err(ModelError::invalid(
                        &cost.id,
                        "rate-generated category cannot also be a fixed trip cost",
                    ));
                }
            }
        }
        Ok(())
    }
}

fn airports<'a>(values: &mut Vec<(&'a Binding, Unit)>, airports: &'a [Airport]) {
    for airport in airports {
        values.extend([
            (&airport.maximum_span, Unit::Meter),
            (&airport.runway, Unit::Meter),
            (&airport.movement_fee, Unit::Dollar),
        ]);
    }
}
