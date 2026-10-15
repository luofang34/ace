# Model reference

## Atmosphere

`atmosphere.isa1976` uses geometric input altitude and converts internally to
geopotential altitude. It covers -2,000 through 86,000 m with the seven
hydrostatic layers of the US Standard Atmosphere 1976, an optional ISA
temperature offset, and Sutherland viscosity.

## Aerodynamics

`aero.parabolic_polar` evaluates dynamic pressure, lift coefficient, induced
factor, drag coefficient, drag, power required, and L/D. Configuration-specific
clean/takeoff/landing inputs determine `CD0`, `e`, and `CLmax`. A power-law
wave-drag increment is optional above critical Mach.

`aero.polar_table` linearly interpolates configuration-specific `CD0`, `CLmax`,
and either Oswald efficiency or induced-drag factor on a strictly increasing
Mach axis. Values outside the axis extrapolate from the nearest interval and
emit a structured `MODEL_EXTRAPOLATION` diagnostic. The axis is the typed model
validity domain with basis `tabulated_data`; each domain names its clean,
takeoff, or landing `applicability_path`. A table cannot be combined with the
power-law wave-drag correction because the table already defines the
Mach-dependent drag behavior. Zero-speed field, native-polar, and drag-chart
screens evaluate the table at Mach 0 and retain extrapolation diagnostics.
Field-length metric validity is `extrapolated` when Mach 0 is outside the
configuration table. Stall, best-glide, minimum-power, and maximum-L/D
reference metrics carry the same per-metric validity, including through native
results and requirements. Performance and climb charts retain both reference
and sampled-point diagnostics. Flight analyses evaluate the table at the
actual operating Mach. Point analysis preflights the requested configuration's
table domain rather than substituting the clean domain. Its reference metrics
carry configuration-aware `metric_validity`: stall speed uses the requested
configuration, while best-glide speed and maximum L/D use clean.

## Propulsion

`propulsion.piston_prop_simple` (`piston_engine`) and
`propulsion.turboprop_simple` (`turboprop_engine`) are shaft engines paired with
a propeller profile. They apply power lapse, throttle, speed-dependent propeller
efficiency, bounded static thrust, and mode BSFC. `power_lapse.model:
density_power` uses `sigma^exponent` for naturally aspirated engines.
`critical_altitude` holds `sigma^exponent_below` (0 for flat rating) up to the
standard-day `critical_altitude`, then continues as
`(sigma / sigma_critical)^exponent_above`. `minimum_power_fraction` is optional
for both. A service or absolute ceiling that exists only because that floor
exceeds the lapsed power emits `CEILING_RELIES_ON_POWER_FLOOR`. Optional
`bsfc.altitude_slope` scales BSFC by `1 + slope * altitude / 1000 m`.

`propulsion.turbofan_simple_deck` applies density/Mach thrust lapse,
installation loss, throttle, and mode-specific TSFC. Profiles warn outside
their Mach/altitude envelopes.

`propulsion.table_deck` bilinearly interpolates per-engine thrust and either
TSFC or specific impulse on strictly increasing Mach and altitude axes for
takeoff, climb, cruise, and economy modes. Matrices are altitude rows by Mach
columns. Engine count, sizing, installation loss, and throttle apply after
interpolation. TSFC uses `T * TSFC`; specific impulse uses `T / (Isp * g0)`.
Queries beyond either axis extrapolate from the nearest interval and emit a
`MODEL_EXTRAPOLATION` warning containing the data bounds. The axes are the
profile's typed validity domain with basis `tabulated_data`.

Schema-v1 table profiles may identify as `turbofan_engine`,
`turbojet_engine`, or `rocket_engine`. Turbojet and rocket types require the
table model and may omit bypass ratio. The shipped J58 profile uses installed
thrust/TSFC cells through Mach 3.3; the XLR99 uses the public 57,000 lbf thrust
anchor and 263 s Isp, producing about 98 kg/s at full throttle.

The canonical propulsion `sizing_factor` scales installed power or thrust.
Sea-level native screens interpolate a table at zero altitude and Mach before
applying engine count, sizing, and installation loss; they do not assume the
first table cell represents sea level.
Automatic refinement also charges 1.15 times the corresponding engine
dry-mass change to operating empty mass.

## OpenVSP geometry refinement

Conventional OpenVSP geometry includes fuselage, wing, horizontal and vertical
tails, engine envelopes, and a resolved propeller when present. Fuselage
length/diameter and horizontal/vertical tail area/arm come from one resolved
aircraft geometry shared with the native wetted-area and structural screens.
Explicit aircraft values take precedence; partial or absent conventional
blocks use versioned statistical correlations with per-scalar provenance.
Explicit turbofan length and diameter values take precedence; missing
turbofan dimensions and piston envelopes use engine dry-mass cube-root
correlations. These dimensions are conceptual inputs, not packaging
substantiation.

The `ace_conventional_geometry` correlation uses the resolved wing area `S`,
span `b`, and any resolved fuselage length `L_f`. Transport means a transport
category or turbofan architecture. Version 1 applies:

| Scalar | Light aircraft | Transport |
|---|---:|---:|
| `L_f` when absent | `0.75 b` | `1.15 b` |
| fuselage diameter when absent | `sqrt(S) / 3` | `sqrt(S) / 5` |
| horizontal-tail area when absent | `0.20 S` | `0.24 S` |
| vertical-tail area when absent | `0.10 S` | `0.12 S` |
| either tail arm when absent | `0.50 L_f` | `0.42 L_f` |

CompGeom evaluates the complete generated model for wetted area. VSPAERO uses
the named `ACE_VSPAERO_LIFTING` set, which contains only the modeled lifting
surfaces; fuselage and propulsion geometry remain in the `.vsp3` artifact but
do not enter the vortex-lattice solve. Conventional script generation creates
fuselage and tail objects only when their resolved topology-backed geometry is
present. OpenVSP results remain an explicit refinement alongside the mandatory
native baseline. The deterministic SVG preview follows the same component
presence rules and scales from the geometry it actually renders.

## Point and envelope performance

The service computes stall, drag, required and available thrust/power, excess
values, climb rate/gradient, best glide, minimum-power speed, maximum speed,
and service/absolute ceilings. Maximum speed uses the high-speed root unless
an explicit operating limit controls first. Ceilings solve maximum climb rate
against configurable class thresholds. Point results report
`fuel_flow_required_kg_s` at the required thrust or shaft power, using the same
formula as mission segments (shaft engines: cruise BSFC times required power
divided by the propeller's cruise `propeller_efficiency`), alongside
`fuel_flow_max_power_kg_s` at full throttle.

A point declared with a longitudinal CG is trimmed by moments about the
horizontal-tail aerodynamic center: the wing carries
`W * (x_tail - x_cg) / (x_tail - x_wing)`, so a forward CG raises stall speed
and induced drag, while climb rate still divides by the full weight. The wing
and tail aerodynamic centers are the same stations the static-margin neutral
point uses: a declared `aircraft.geometry.wing.aerodynamic_center`, or else a
fuselage-length layout correlation that is reported as
`CORRELATED_GEOMETRY_STATION`.

Performance results include `metric_validity` for maximum level speed, service
ceiling, and absolute ceiling. An interior root is `valid`; an interior root
that uses a model outside its registered envelope is `extrapolated`. A maximum
speed pinned to a default search cap is `boundary_limited`; explicit aircraft
speed limits are valid declared limits. Ceilings are solved as physical roots
of the climb threshold by stepping up through the whole atmosphere and
bisecting the first sign change; `physical_service_ceiling_m` and
`physical_absolute_ceiling_m` report them uncut, `extrapolated` where a root
lies beyond an engine profile's declared envelope. Like POH figures,
`service_ceiling_m` and `absolute_ceiling_m` are those roots cut off at the
lowest of `aircraft.limits.maximum_operating_altitude` and the engine's
`limits.maximum_altitude`; `ceiling_limited_by` names the limit when it binds,
and a binding declared limit is a valid published value. When the climb
threshold is still met at 86 km, or not met at sea level, the physical ceilings
are omitted, their validity is `unsupported`, and `CEILING_UNAVAILABLE`
explains why; the published ceiling then exists only if a declared limit inside
the atmosphere cuts it off. `performance.service_ceiling` requirements judge
the published figure and `performance.physical_service_ceiling` the uncut climb
capability. Climb-rate optima at the 1.05 V_stall search floor are stall-margin
constrained, not boundary-limited.

Declared cruise Mach and true airspeed remain inputs for reporting. Achieved
cruise Mach and true airspeed evaluate installed capability at every cruise
segment using its altitude and mission mass. The aggregate comes from the
condition with minimum excess power, and `cruise_feasible` is true only when
every declared cruise condition closes with nonnegative excess power and stays
within declared aircraft speed, Mach, and altitude limits. Unsupported or
out-of-limit conditions remain explicit diagnostics and make cruise feasibility
false. Requirements bind to the achieved metrics; declared cruise metric names
are rejected with `DECLARED_METRIC_NOT_BINDABLE` and their achieved replacement.

## Mission

Quasi-steady segments integrate fuel and enforce mass continuity. Cruise and
loiter use required drag/power at decreasing mass. `energy_climb` integrates
`m[g Δh + Δ(V²)/2] / Pexcess` through fixed midpoint steps, updating current
mass and installed fuel flow at every step. It rejects nonpositive excess
power and nonmonotonic schedules. Legacy `climb` uses an explicitly
low-fidelity constant rate capped at 50 m/s; descent retains a simplified
rate.

Synthesis `energy_path` phases integrate a straight prescribed path with exact
`sin(gamma)`/`cos(gamma)` force balance, but are registered only for flight-path
angles within +/-10 deg and altitudes 0-20 km, so `path_distance` must be at
least the altitude change divided by tan 10 deg (about 5.7 times it). Steeper
paths are rejected rather than integrated outside that domain.

Completed missions publish `mission.landing_fuel`. Landing fuel below 5% of
the aircraft's maximum fuel capacity emits the advisory
`LOW_LANDING_FUEL`; incomplete missions publish neither the achieved metric
nor that advisory. Explicit landing-fuel floors remain ordinary requirements.

Feasibility separately checks legacy climb, energy climb, cruise, loiter, and
reserve operating points at their declared power or thrust fractions. Each
point must retain a 3% installed-reference-power reserve.

## Conceptual structural screening

The native structural screen estimates ultimate wing-root bending, required
spar-cap area, spar-cap packaging ratio, horizontal and vertical tail-volume
coefficients, and aspect-ratio validity. It assumes a 1.5 ultimate factor,
240 MPa cap allowable, and a 9%-chord spar depth. Tail-volume coefficients use
the same resolved tail areas and independent arms supplied to geometry
backends.

Passing this screen means the concept is suitable for further study. Detailed
loads, joints, buckling, fatigue, flutter, aeroelasticity, and certification
substantiation remain outside the model.

## Machine-readable validity

Native analysis provenance publishes typed validity domains for the atmosphere,
polar and optional wave-drag increment, resolved propulsion profile,
field-performance screen (0-4.5 km field elevation), and selected structural screen. Bounds use
canonical SI units and identify whether they come from a published
specification, the model form, resolved profile data, tabulated data, or a
screening assumption. Human-readable `validity_range` text remains part of
provenance.

When one model has configuration-specific domains, `model_id` and
`applicability_path` jointly identify each domain. Study evidence preserves
those scoped domains independently.

## Constraints and payload-range

Constraint diagrams sample P/W vs W/S or T/W vs W/S for stall, cruise, climb,
and approximate takeoff. Payload-range uses representative cruise fuel flow,
MTOW, OEW, maximum payload, maximum fuel, and an explicit reserve fraction.
Both are fidelity level 0 and label their limitations.

Breguet jet estimates use the cruise table's actual fuel basis. Their
assumption ledger names TSFC or specific impulse consistently with the
selected table mode.
