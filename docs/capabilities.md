# Capability manifest

The CLI command `aex capabilities --format json` and MCP tool `get_capabilities` return the same versioned manifest.

## Documents

`aircraft`, `mission`, `requirements`, `profile`, `scenario`, `study`

## Profile types

| Type | Role |
| --- | --- |
| `piston_engine` | `engine` |
| `turbofan_engine` | `engine` |
| `propeller` | `propeller` |

## Aerodynamic configurations

`clean`, `takeoff`, `landing`

## Mission segments

| Type | Legal fields |
| --- | --- |
| `start_and_taxi` | `duration` (required), `altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `fixed_time` | `duration` (required), `altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `fixed_fuel` | `fuel_fraction` (exactly_one: fuel), `fuel_mass` (exactly_one: fuel) |
| `payload_drop` | `payload_mass` (required) |
| `takeoff` | `duration` (required), `altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `climb` | `target_altitude` (required), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `cruise` | `distance` (required), `altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `loiter` | `duration` (required), `altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `descent` | `target_altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `landing` | `duration` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |
| `reserve` | `duration` (required), `altitude` (optional), `indicated_airspeed` (at_most_one: speed), `true_airspeed` (at_most_one: speed), `mach` (at_most_one: speed), `power_fraction` (at_most_one: throttle), `thrust_fraction` (at_most_one: throttle) |

Fields in the same `at_most_one` group are mutually exclusive. Fields in an `exactly_one` group require one and only one representation. Unlisted fields are rejected.

A declared power or thrust fraction of zero means engine off and produces zero modeled propulsion output and fuel flow.

Power/thrust fractions on climb, cruise, loiter, and reserve constrain the mission-power feasibility screen. Quasi-steady cruise/loiter fuel burn follows the aerodynamic power required and is not scaled directly by throttle.

## Requirement metrics

| Metric | Source | Bindable | Unit | Replacement |
| --- | --- | --- | --- | --- |
| `mission.payload_mass` | `declared` | yes | `kg` | — |
| `performance.achieved_cruise_true_airspeed` | `achieved` | yes | `m/s` | — |
| `mission.completed_distance` | `achieved` | yes | `m` | — |
| `performance.service_ceiling` | `achieved` | yes | `m` | — |
| `performance.stall_speed_landing` | `achieved` | yes | `m/s` | — |
| `performance.achieved_cruise_mach` | `achieved` | yes | `1` | — |
| `performance.minimum_cruise_excess_power` | `achieved` | yes | `W` | — |
| `performance.cruise_feasible` | `achieved` | yes | `1` | — |
| `performance.takeoff_field_length` | `achieved` | yes | `m` | — |
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
- `aero.wave_drag_power_law`
- `propulsion.piston_prop_simple`
- `propulsion.turbofan_simple_deck`
- `performance.field_length_simple`
- `structures.conventional_conceptual_screen`
- `structures.blended_wing_conceptual_screen`

Strict-warning decisions are listed in [the generated warning policy](strict-warning-policy.md).
