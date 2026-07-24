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

## Point and envelope performance

The service computes stall, drag, required and available thrust/power, excess
values, climb rate/gradient, best glide, minimum-power speed, maximum speed,
and service/absolute ceilings. Maximum speed uses the high-speed root unless
an explicit operating limit controls first. Ceilings solve maximum climb rate
against configurable class thresholds.

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

## Constraints and payload-range

Constraint diagrams sample P/W vs W/S or T/W vs W/S for stall, cruise, climb,
and approximate takeoff. Payload-range uses representative cruise fuel flow,
MTOW, OEW, maximum payload, maximum fuel, and an explicit reserve fraction.
Both are fidelity level 0 and label their limitations.
