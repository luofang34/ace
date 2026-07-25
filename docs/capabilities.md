# Capability manifest

The CLI command `aex capabilities --format json` and MCP tool `get_capabilities` return the same versioned manifest.

## Documents

`aircraft`, `mission`, `requirements`, `profile`, `scenario`, `study`

## Profile types

| Type | Role |
| --- | --- |
| `piston_engine` | `engine` |
| `turbofan_engine` | `engine` |
| `turbojet_engine` | `engine` |
| `rocket_engine` | `engine` |
| `propeller` | `propeller` |

## Aerodynamic configurations

`clean`, `takeoff`, `landing`

## Mission segments

Initial-state fields: `altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `fuel_fraction` (at_most_one: fuel), `fuel_mass` (at_most_one: fuel).

Energy-schedule point fields: `altitude` (required), `indicated_airspeed` (exactly_one: speed), `true_airspeed` (exactly_one: speed), `mach` (exactly_one: speed).

| Type | Legal fields |
| --- | --- |
| `start_and_taxi` | `duration` (required), `altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `fixed_time` | `duration` (required), `altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `fixed_fuel` | `fuel_fraction` (exactly_one: fuel), `fuel_mass` (exactly_one: fuel) |
| `payload_drop` | `payload_mass` (required) |
| `takeoff` | `duration` (required), `altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `climb` | `target_altitude` (required), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `energy_climb` | `schedule` (required), `power_fraction` (exactly_one: throttle), `thrust_fraction` (exactly_one: throttle) |
| `cruise` | `distance` (required), `altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `loiter` | `duration` (required), `altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `descent` | `target_altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `landing` | `duration` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `reserve` | `duration` (required), `altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |

Fields in the same `at_most_one` group are mutually exclusive. Fields in an `exactly_one` group require one and only one representation. Unlisted fields are rejected.

A declared power or thrust fraction of zero means engine off and produces zero modeled propulsion output and fuel flow.

Energy-climb schedules use at least two strictly increasing altitude points, one speed representation per point, and exactly one segment throttle setting. The solver uses fixed midpoint steps and rejects nonpositive excess power rather than clamping it.

Legacy `climb` remains a low-fidelity constant-rate model capped at 50 m/s.

Power/thrust fractions on climb, energy_climb, cruise, loiter, and reserve constrain the mission-power feasibility screen. Quasi-steady cruise/loiter fuel burn follows the aerodynamic power required and is not scaled directly by throttle.

## Requirement templates

All shipped templates are conceptual screens, not certification findings.

Transport OEI second-segment gradients are regulatory-derived from [14 CFR 25.121(b)](https://www.ecfr.gov/current/title-14/section-25.121); the implemented calculation remains a conceptual screen.

| Template | Version | Category | Parameter | Items |
| --- | --- | --- | --- | --- |
| `light_aircraft_conceptual` | 1 | normal-category light aircraft | — | `stall_speed_landing` = 61 kt [soft; designer_default]<br>`takeoff_field_length` = 2500 ft [soft; designer_default]<br>`landing_field_length` = 2500 ft [soft; designer_default]<br>`all_engine_climb_gradient` = 0.05 [soft; designer_default]<br>`reserve_duration` = 45 min [soft; designer_default] |
| `transport_conceptual` | 1 | transport | `engine_count` (2/3/4) | `stall_speed_landing` = 150 kt [soft; designer_default]<br>`takeoff_field_length` = 11000 ft [soft; designer_default]<br>`landing_field_length` = 8000 ft [soft; designer_default]<br>`reserve_duration` = 30 min [soft; designer_default]<br>`oei_second_segment_climb_gradient` = 2:0.024/3:0.027/4:0.030 [hard; regulatory_derived] |

## Requirement metrics

| Metric | Source | Bindable | Unit | Replacement |
| --- | --- | --- | --- | --- |
| `mission.payload_mass` | `declared` | yes | `kg` | — |
| `performance.achieved_cruise_true_airspeed` | `achieved` | yes | `m/s` | — |
| `mission.completed_distance` | `achieved` | yes | `m` | — |
| `mission.landing_fuel` | `achieved` | yes | `kg` | — |
| `mission.reserve_duration` | `achieved` | yes | `s` | — |
| `performance.service_ceiling` | `achieved` | yes | `m` | — |
| `performance.stall_speed_landing` | `achieved` | yes | `m/s` | — |
| `performance.achieved_cruise_mach` | `achieved` | yes | `1` | — |
| `performance.minimum_cruise_excess_power` | `achieved` | yes | `W` | — |
| `performance.cruise_feasible` | `achieved` | yes | `1` | — |
| `performance.takeoff_field_length` | `achieved` | yes | `m` | — |
| `performance.landing_field_length` | `achieved` | yes | `m` | — |
| `performance.all_engine_climb_gradient` | `achieved` | yes | `1` | — |
| `performance.oei_second_segment_climb_gradient` | `achieved` | yes | `1` | — |
| `performance.full_payload_range` | `achieved` | yes | `m` | — |
| `performance.zero_payload_ferry_range` | `achieved` | yes | `m` | — |
| `performance.cruise_mach` | `declared` | no | `1` | `performance.achieved_cruise_mach` |
| `performance.cruise_true_airspeed` | `declared` | no | `m/s` | `performance.achieved_cruise_true_airspeed` |

Operators: `ge`, `le`, `eq`. Severities: `hard`, `soft`, `report_only`.

## Backends

- `native`
- `openvsp`

## Registered model domains

- `atmosphere.isa1976`
- `aero.parabolic_polar`
- `aero.polar_table`
- `aero.wave_drag_power_law`
- `propulsion.piston_prop_simple`
- `propulsion.turbofan_simple_deck`
- `propulsion.table_deck`
- `performance.field_length_simple`
- `structures.conventional_conceptual_screen`
- `structures.blended_wing_conceptual_screen`

Strict-warning decisions are listed in [the generated warning policy](strict-warning-policy.md).
