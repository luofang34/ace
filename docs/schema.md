# Schema and units

Documents are YAML or JSON mappings with `schema_version: 1` and one envelope:
`aircraft`, `mission`, `requirements`, `profile`, `scenario`, or `study`.

Physical values must include a unit:

```yaml
maximum_takeoff_mass: 1111 kg
area: 174 ft^2
cruise_speed: 122 kt
distance: 350 nmi
```

Bare numbers are reserved for dimensionless Mach, coefficients, ratios,
counts, and enumerated values. Unsupported or incompatible units return a
typed `Quantity`/validation error rather than guessing.

The resolver normalizes SI values and records scalar sources in an assumptions
ledger. Interface quantity objects contain `value`, `unit`, `display_value`,
and `display_unit`. The adjacent `aircraft-explorer.yaml` project selects
`default_unit_system: si|aviation_us`; an explicit request override takes
precedence, and scenarios without a project use SI. Aviation-US presentation
uses kt, ft, lb, and nmi for speeds, lengths/altitudes, masses, and semantic
distances. The canonical `value` and `unit` remain SI in every system and are
the only quantity fields used by persisted identities. Ledger units are
assigned only to registered physical quantity paths; names, identifiers,
topology vocabulary, requirement metadata, and unknown string fields remain
verbatim text with `unit: null`.

Scenario documents may contain an `overrides` map. The design MCP tools use
this map as an editable overlay instead of rewriting aircraft, mission, or
requirements documents:

```yaml
scenario:
  id: c172_experiment
  name: C172 Experiment
  aircraft: aircraft.yaml
  mission: mission.yaml
  requirements: requirements.yaml
  overrides:
    aircraft.geometry.wing.area: 18.0 m^2
    aircraft.geometry.wing.aspect_ratio: "8.4"
    aircraft.geometry.wing.span: 12.296341 m
    aircraft.propulsion.sizing_factor: "1.10"
```

Call-specific overrides take precedence over persisted scenario overrides.
Both are validated against the same typed canonical models.
Sequence entries are addressed by their stable `id`, not by array position:

```yaml
scenario:
  overrides:
    mission.segments.outbound_cruise.mach: "0.58"
```

Wing `area`, `span`, and `aspect_ratio` are individually optional, but every
aircraft supplies at least two. The resolver derives the missing value from
`aspect_ratio = span² / area`. A fully declared rounded triple may differ by
at most 0.5%; the resolved aspect ratio is normalized from area and span.
Larger discrepancies return `INCONSISTENT_WING_PLANFORM`. A sweep of one or
two planform fields completes the dependent values before resolution while
keeping the requested sweep variables unchanged in its result rows.
Aspect-ratio linear sweep bounds are dimensionless bare numbers; `1` is also
accepted as an explicit unit.

Each clean, takeoff, or landing aerodynamic configuration may add a
Mach-dependent polar table while retaining the schema-version-1 scalar fields:

```yaml
clean:
  cd0: 0.012
  oswald_efficiency: 0.72
  cl_max: 1.05
  polar_table:
    mach: [0.0, 0.85, 1.15, 3.2]
    cd0: [0.012, 0.013, 0.035, 0.0145]
    cl_max: [1.05, 1.00, 0.85, 0.60]
    oswald_efficiency: [0.72, 0.68, 0.50, 0.34]
```

The Mach axis requires at least two finite, nonnegative, strictly increasing
breakpoints. `cd0`, `cl_max`, and the induced schedule have the same length and
positive finite values. Exactly one of `oswald_efficiency` or
`induced_drag_factor` is present. Oswald efficiency is bounded by `(0, 1]`.
Inside the axis the model linearly interpolates; outside it the nearest
interval extrapolates with `MODEL_EXTRAPOLATION`. `polar_table` and the legacy
`wave_drag` correction are mutually exclusive. Typed table validity domains
carry an `applicability_path` for the clean, takeoff, or landing configuration,
so configurations with different Mach support remain distinguishable.
Consumers that intentionally query Mach 0 retain the resulting diagnostic;
field-length results also mark the corresponding metric validity as
`extrapolated`. Other Mach-0 reference metrics, including stall, best-glide,
minimum-power, and maximum-L/D values, publish matching validity metadata.
Stored evidence treats `(model_id, applicability_path)` as the domain identity.

Mission segments with `type: payload_drop` require `payload_mass`. The
simulator removes that mass without recording fuel burn, enabling an explicit
payload-delivery and empty-return mission.

Missions may declare an additive schema-version-1 `initial_state`:

```yaml
mission:
  initial_state:
    altitude: 45000 ft
    mach: 0.65
    fuel_mass: 8500 kg
```

Altitude is optional. At most one of `indicated_airspeed`, `true_airspeed`, or
`mach` and at most one of `fuel_mass` or `fuel_fraction` may be present. Fuel
fraction is a fraction of the usable load after tank capacity and the
MTOW/OEW/payload limit are applied. Declared fuel, altitude, effective speed,
Mach, and initial mass are checked against aircraft limits and registered
model domains before simulation. A declared speed is inherited by later
segments without their own speed; an explicit segment speed supersedes it.
When `initial_state` is absent, missions retain the ground, full-usable-fuel,
engine-class speed defaults.

Mission fields are defined per segment type by the generated
[`capabilities.md`](capabilities.md) matrix. Unlisted and arbitrary keys return
`UNSUPPORTED_SEGMENT_FIELD`; fields in the same speed or throttle group are
mutually exclusive. A timed segment may declare `altitude`, which controls its
atmosphere and propulsion operating point and becomes the altitude inherited
by the following segment. Schema-version-1 documents that declare one speed
and one throttle representation retain their existing interpretation.
Explicit `power_fraction: 0` or `thrust_fraction: 0` means engine off: modeled
propulsion output and fuel flow are exactly zero, and the point is not subject
to a powered mission-reserve check. Engine-off climb to a higher altitude is
rejected with `ENGINE_OFF_CLIMB_UNSUPPORTED`.

`energy_climb` represents coupled climb and acceleration with an ordered
`schedule`. Each of its two or more points declares `altitude` and exactly one
of `indicated_airspeed`, `true_airspeed`, or `mach`; the segment declares
exactly one power or thrust fraction. Altitude must increase strictly, true
airspeed must not decrease, and the first altitude must match the propagated
mission state. The deterministic solver integrates
`g Δh + Δ(V²)/2` with fixed midpoint steps at current mass. Nonpositive excess
power returns `NONPOSITIVE_EXCESS_POWER`; schedules are never clamped.
Legacy `climb` retains its conceptual constant-rate method with a 50 m/s cap.

Aerodynamic and propulsion diagnostics produced while evaluating a mission
segment appear both on that segment and in the mission warning channel.
Diagnostic paths use the stable segment ID, for example
`mission.segments.supersonic_cruise.condition.mach`. Repeated integration
steps deduplicate the same model code and source path within one segment;
matching warnings from different segment IDs remain distinct. Strict mission
analysis therefore applies the same model-warning policy as point analysis.
Production warning codes are registered enum variants but retain their
schema-version-1 uppercase strings. Unknown codes in stored diagnostics remain
readable and advisory. The complete generated table is in
[`strict-warning-policy.md`](strict-warning-policy.md).

`aircraft.geometry.wing.center_body_edge_sweep` is an optional angle for
tailless blended-wing-body geometry. It denotes the positive magnitude of
opposed center-body leading- and trailing-edge sweeps. OpenVSP translates it
to the backend's quarter-chord representation without exposing backend
parameter identifiers.

Turbofan profiles may provide `dimensions.overall_length` and
`dimensions.maximum_diameter`. OpenVSP uses them for the engine envelope and
retains conservative defaults when they are absent.

Turbofan profiles select either the legacy simple-deck parameters or a
`table_deck`. A table declares `axes.mach`, quantity-valued `axes.altitude`,
and all four `modes`: `takeoff`, `climb`, `cruise`, and `economy`. Each mode
contains a `thrust` matrix in `N` and exactly one fuel matrix: `tsfc` in
`kg/N/hr` or `specific_impulse` in `s`. Every matrix uses altitude rows and
Mach columns, matching the strictly increasing axes, and every cell must be
finite and positive. Table profiles do not require simple-deck lapse, TSFC, or
author-declared limit fields.

Resolved piston, turbofan, and propeller parameters are checked against
registered advisory typical ranges. Every violation emits
`PARAMETER_OUTSIDE_TYPICAL` with structured value, inclusive bounds, and unit
context. Non-strict resolution remains available for explicit surrogate
profiles; `--strict` promotes the warning to failure. Typical ranges identify a
model-fit concern and are not a substitute for a model validity envelope.

## Aircraft topology

`aircraft.topology` is an optional, backend-neutral component graph. Components
have stable IDs, registered kinds, optional reusable definitions, counts,
analysis roles, and data parameters. Relationships express attachment,
symmetry, repetition, alignment, parallelism, continuity, and load paths.
Backend geometry IDs, meshes, and solver entities are not schema data.

When topology is absent, schema-version-1 aircraft retain their behavior
through a resolved graph inferred from `aircraft.configuration`, engine count,
and propeller presence. An explicit graph takes precedence over the legacy
configuration label.

Canonical component kinds include fuselage, wing, horizontal and vertical
tails, canard, lifting body, boom, engine, propeller, fuel system, and payload.
A backend advertises the subset it can analyze. A canonical graph outside that
subset resolves normally, but feasibility returns `status: "unsupported"`
with code `UNSUPPORTED_BACKEND_TOPOLOGY`, unsupported kinds, and any unsupported
count, parameter, or relationship-endpoint features. Unknown vocabulary
remains a validation error. Component definitions and roles are descriptive;
backend-relevant counts, parameters, and relationships must be consumed or
rejected before analysis. A refinement descriptor may delegate a relationship
outside its disciplines only when the mandatory native baseline consumes it.

## Study and evidence documents

A study is portable when its baseline is either a relative `scenario_path` or
an embedded schema-version-1 document set. Exactly one baseline form is
required. Variables identify canonical dotted paths, value sets, and a
continuous, integer, or categorical kind. Objectives and constraints use
stable IDs and metric names. Analysis and search policies are data:

```yaml
schema_version: 1
study:
  id: local-wing-trade
  name: Local wing trade
  baseline: { scenario_path: scenario.yaml }
  variables:
    - id: wing_area
      path: aircraft.geometry.wing.area
      kind: continuous
      values: ["15 m^2", "17 m^2"]
  objectives:
    - id: minimize_fuel
      metric: mission.total_fuel
      direction: minimize
```

Omitted policy fields select native screening, grid search, three refinement
candidates, 256 evaluations, population 24, 12 generations, mutation rate
0.15, and seed zero. C172 and B777 reference studies pin explicit policies in
their example directories.

Candidate, evaluation, and archive IDs are SHA-256 content identities with
`candidate_`, `eval_`, and `archive_` prefixes. Candidate identity covers the
baseline digest and normalized parameter map; accepted unit aliases and
equivalent SI quantities canonicalize before hashing. Evaluation identity
covers its candidate, inputs, status, analysis method, results, diagnostics,
and provenance. Archive identity covers the study, evaluator signature,
completion state, candidate descriptors, evaluation references, selection,
optimizer checkpoint, candidate outcomes, scores, and Pareto membership.

Evidence records separate analysis identity, metrics and constraint results,
and provenance. File storage shards immutable evaluation and archive JSON by
digest prefix under `evaluations/` and `archives/`. An identical write is
idempotent; changed content cannot replace an existing identity. Candidate
directories are not part of the storage contract.

Result and evidence provenance may include `validity_domains`, a
machine-readable list keyed by `model_id`. Each bound names a typed variable,
canonical SI unit, optional minimum and maximum with explicit inclusivity, and
a basis such as `published_specification`, `model_form`, `resolved_profile`,
`tabulated_data`, or `screening_assumption`. The prose `validity_range` remains
available for human readers. Omitted typed domains default to an empty list and
empty lists are not serialized, preserving schema-version-1 stored-record
identities.

Scenario resolution preflights declared altitudes, speeds, Mach numbers,
masses, and applicable aircraft limits against every model that consumes
them. All breaches are returned together as `MODEL_DOMAIN_UNSUPPORTED`, ordered
by path and model ID. Each violation carries its typed variable, declared value
and unit, registered bounds and inclusivity, bound unit, and domain basis.
Validation and analysis therefore reject the same unsupported scenario before
simulation. A model bound is not applied to a declaration that the model does
not consume; for example, a sea-level field-performance assumption does not
bound cruise altitude.

Non-finite declarations are invalid input and return `NON_FINITE_VALUE` before
domain comparison. Mission simulation, payload-range analysis, and preflight
share altitude propagation and the true-airspeed → Mach → indicated-airspeed
selection rules; indicated airspeed is converted with the atmosphere at the
segment operating altitude. Domain registrations carry their consuming model
role independently of the human-readable model ID, so profile IDs cannot
change which declarations a model receives.

Bounded performance metrics carry additive `metric_validity` entries with
`valid`, `extrapolated`, or `boundary_limited` status. Boundary-limited entries
also identify the search or model boundary. Requirement evaluations expose
`status: pass|fail|indeterminate`; the compatibility field `passed` is a
boolean for pass/fail and `null` for indeterminate. A boundary-limited actual
always makes its requirement indeterminate. Omitted validity metadata defaults
to valid, and stored boolean-only requirement results remain readable.
Indeterminate hard constraints are infeasible and retain positive normalized
violation in study evidence.

Point-performance results also carry additive `metric_validity` entries for
stall speed, best-glide speed, and maximum L/D. Mach-zero table extrapolation
is retained in both this map and the point warning channel. Stored point
results without the map remain readable and default to an empty map.

Performance summaries preserve the compatibility `cruise_mach` declaration
and add explicit declared Mach/TAS, achieved cruise Mach/TAS, minimum cruise
excess power, all-condition cruise feasibility, and per-segment conditions.
Per-segment achieved Mach/TAS are nullable when installed capability has no
level-flight solution. Any missing or unavailable declared condition makes
aggregate achieved metrics and minimum excess power unavailable, rather than
selecting a stronger partial subset. Omitted additive fields default to `null`
or an empty list for stored schema-version-1 results. Requirement documents
must use
`performance.achieved_cruise_mach` or
`performance.achieved_cruise_true_airspeed`; the legacy declared names are
reporting inputs and are not bindable.

Completed mission results add `landing_fuel` as a canonical mass quantity.
The bindable achieved metric `mission.landing_fuel` uses that value. Incomplete
missions omit the metric; a hard landing-fuel floor is therefore unevaluable
and cannot pass.

Archive workflow data is additive and defaults to empty, so schema-version-1
archives without it remain valid. A checkpoint records the completed
generation, deterministic random-number state after population construction,
and current population.
Outcomes bind one candidate to one evaluation with feasibility, normalized
hard-constraint violation, objective values, and ranking score. All references
must be unique archive members with finite scores.

Grid search uses document variable order and value order. Evolutionary search
uses the declared seed, population, generation limit, mutation rate, and
maximum evaluation count. Immutable checkpoints allow execution to resume
without rewriting an archive or reevaluating a known candidate. Pareto sets
contain feasible candidates only; hard-infeasible candidates cannot outrank a
feasible candidate. Study-specific constraints with the same ID as a baseline
requirement replace that requirement in the study evidence, allowing a study
to strengthen severity without creating duplicate constraint identities.
Conditional grid combinations that resolve to the same descriptor are
deduplicated before they count toward the maximum evaluation limit.
The `preserve_baseline_mass_closure` derivation applies propulsion dry-mass
growth to both operating empty mass and maximum takeoff mass, while fuel
capacity changes adjust maximum takeoff mass by the same amount.

Archive consumers cross-check every outcome against the referenced candidate
and immutable evidence record, including study ID and recorded feasibility.
Queries do not infer a revision when multiple study, baseline, or evaluator
signatures share one study ID; callers select the returned archive ID.
