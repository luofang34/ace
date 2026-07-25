# Model reference

## Atmosphere

`atmosphere.isa1976` uses geometric input altitude and converts internally to
geopotential altitude. It covers -2,000 through 20,000 m, the tropospheric
lapse layer and isothermal lower stratosphere, and Sutherland viscosity.

## Aerodynamics

`aero.parabolic_polar` evaluates dynamic pressure, lift coefficient, induced
factor, drag coefficient, drag, power required, and L/D. Configuration-specific
clean/takeoff/landing inputs determine `CD0`, `e`, and `CLmax`. A power-law
wave-drag increment is optional above critical Mach.

## Propulsion

`propulsion.piston_prop_simple` applies density-ratio power lapse, throttle,
speed-dependent propeller efficiency, bounded static thrust, and profile BSFC.

`propulsion.turbofan_simple_deck` applies density/Mach thrust lapse,
installation loss, throttle, and mode-specific TSFC. Profiles warn outside
their Mach/altitude envelopes.

The canonical propulsion `sizing_factor` scales installed power or thrust.
Automatic refinement also charges 1.15 times the corresponding engine
dry-mass change to operating empty mass.

## OpenVSP geometry refinement

Conventional OpenVSP geometry includes fuselage, wing, horizontal and vertical
tails, engine envelopes, and a resolved propeller when present. Explicit
turbofan length and diameter values take precedence; missing turbofan
dimensions and piston envelopes use engine dry-mass cube-root correlations.
These dimensions are visualization and low-order wetted-area inputs, not
packaging substantiation.

CompGeom evaluates the complete generated model for wetted area. VSPAERO uses
the named `ACE_VSPAERO_LIFTING` set, which contains only the modeled lifting
surfaces; fuselage and propulsion geometry remain in the `.vsp3` artifact but
do not enter the vortex-lattice solve. OpenVSP results remain an explicit
refinement alongside the mandatory native baseline.

## Point and envelope performance

The service computes stall, drag, required and available thrust/power, excess
values, climb rate/gradient, best glide, minimum-power speed, maximum speed,
and service/absolute ceilings. Maximum speed uses the high-speed root unless
an explicit operating limit controls first. Ceilings solve maximum climb rate
against configurable class thresholds.

Performance results include `metric_validity` for maximum level speed, service
ceiling, and absolute ceiling. An interior root is `valid`; an interior root
that uses a model outside its registered envelope is `extrapolated`; a result
pinned to the atmosphere or a default search cap is `boundary_limited`.
Explicit aircraft operating limits are treated as valid declared limits rather
than artificial solver caps. A boundary-limited value is not evidence that the
physical threshold occurs at that value.

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
loiter use required drag/power at decreasing mass. Climb and descent use
explicitly simplified rates and carry low-fidelity warnings.

Feasibility separately checks climb, cruise, loiter, and reserve operating
points at their declared power or thrust fractions. Each point must retain a
3% installed-reference-power reserve.

## Conceptual structural screening

The native structural screen estimates ultimate wing-root bending, required
spar-cap area, spar-cap packaging ratio, horizontal and vertical tail-volume
coefficients, and aspect-ratio validity. It assumes a 1.5 ultimate factor,
240 MPa cap allowable, and a 9%-chord spar depth.

Passing this screen means the concept is suitable for further study. Detailed
loads, joints, buckling, fatigue, flutter, aeroelasticity, and certification
substantiation remain outside the model.

## Machine-readable validity

Native analysis provenance publishes typed validity domains for the atmosphere,
polar and optional wave-drag increment, resolved propulsion profile,
sea-level field-performance screen, and selected structural screen. Bounds use
canonical SI units and identify whether they come from a published
specification, the model form, resolved profile data, or a screening
assumption. Human-readable `validity_range` text remains part of provenance.

## Constraints and payload-range

Constraint diagrams sample P/W vs W/S or T/W vs W/S for stall, cruise, climb,
and approximate takeoff. Payload-range uses representative cruise fuel flow,
MTOW, OEW, maximum payload, maximum fuel, and an explicit reserve fraction.
Both are fidelity level 0 and label their limitations.
