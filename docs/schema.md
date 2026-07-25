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
and `display_unit`. Range fields use metres for `value` and nautical miles for
display by default.

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

Mission segments with `type: payload_drop` require `payload_mass`. The
simulator removes that mass without recording fuel burn, enabling an explicit
payload-delivery and empty-return mission.

`aircraft.geometry.wing.center_body_edge_sweep` is an optional angle for
tailless blended-wing-body geometry. It denotes the positive magnitude of
opposed center-body leading- and trailing-edge sweeps. OpenVSP translates it
to the backend's quarter-chord representation without exposing backend
parameter identifiers.

Turbofan profiles may provide `dimensions.overall_length` and
`dimensions.maximum_diameter`. OpenVSP uses them for the engine envelope and
retains conservative defaults when they are absent.

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

Archive workflow data is additive and defaults to empty, so schema-version-1
archives without it remain valid. A checkpoint records the completed
generation, deterministic random-number state, and current population.
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
